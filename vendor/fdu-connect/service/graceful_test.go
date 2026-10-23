package service

import (
	"context"
	"errors"
	"net"
	"sync/atomic"
	"testing"
	"time"

	"github.com/akirasalvare/fdu-connect/dial"
	"github.com/akirasalvare/fdu-connect/internal/zcdns"
)

// fakeStack simulates the user-space RVPN stack: a TCP "handshake" that can be
// fast, slow, failing or cancelled by the context.
type fakeStack struct {
	delay time.Duration
	err   error
	// typedNilWithErr mimics gonet.DialContextTCP, which returns a typed nil
	// *gonet.TCPConn together with an error.
	typedNilWithErr bool
}

func (f *fakeStack) Run()                             {}
func (f *fakeStack) SetupResolve(_ zcdns.LocalServer) {}

func (f *fakeStack) DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error) {
	client, server := net.Pipe()
	select {
	case <-time.After(f.delay):
		if f.typedNilWithErr {
			_ = client.Close()
			_ = server.Close()
			var typedNil *net.TCPConn
			return typedNil, errors.New("dial failed after partial handshake")
		}
		if f.err != nil {
			_ = client.Close()
			_ = server.Close()
			return nil, f.err
		}
		// Simulate the first byte arriving from the remote host (e.g. the SSH
		// banner) so the deferred connection can be read by the tests.
		go func() {
			_, _ = server.Write([]byte{'x'})
		}()
		return client, nil
	case <-ctx.Done():
		_ = client.Close()
		_ = server.Close()
		return nil, ctx.Err()
	}
}

func (f *fakeStack) DialTCP(addr *net.TCPAddr) (net.Conn, error) {
	return f.DialTCPContext(context.Background(), addr)
}

func (f *fakeStack) DialUDP(addr *net.UDPAddr) (net.Conn, error) { return nil, errors.New("no udp") }

func newTestDialer(delay time.Duration, err error) *dial.Dialer {
	// alwaysUseVPN = true forces the stack path, which is what the SOCKS5 server
	// uses for intranet destinations.
	return dial.NewDialer(&fakeStack{delay: delay, err: err}, nil, nil, true, "")
}

func TestDialWithGraceFastPathReturnsRealConn(t *testing.T) {
	d := newTestDialer(20*time.Millisecond, nil)
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 5*time.Second, 2*time.Second)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if _, ok := conn.(*gracefulConn); ok {
		t.Fatalf("fast dial should return the real connection")
	}
	conn.Close()
}

func TestDialWithGraceSlowDialIsDeferred(t *testing.T) {
	d := newTestDialer(300*time.Millisecond, nil)
	start := time.Now()
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 3*time.Second, 50*time.Millisecond)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	elapsed := time.Since(start)
	if elapsed > 250*time.Millisecond {
		t.Fatalf("reply was not sent early, took %s", elapsed)
	}
	if _, ok := conn.(*gracefulConn); !ok {
		t.Fatalf("slow dial should be deferred, got %T", conn)
	}
	// First Read must block until the tunnel finished the handshake.
	buf := make([]byte, 1)
	if _, err := conn.Read(buf); err != nil {
		t.Fatalf("deferred conn read failed: %v", err)
	}
	conn.Close()
}

func TestDialWithGracePropagatesDialError(t *testing.T) {
	d := newTestDialer(200*time.Millisecond, errors.New("network is unreachable"))
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 5*time.Second, 50*time.Millisecond)
	if err != nil {
		t.Fatalf("expected deferred conn, got error: %v", err)
	}
	if _, err := conn.Read(make([]byte, 1)); err == nil {
		t.Fatalf("expected dial error on first read")
	}
	conn.Close()
}

func TestDialWithGraceCloseCancelsDial(t *testing.T) {
	d := newTestDialer(10*time.Second, nil)
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 30*time.Second, 50*time.Millisecond)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	_ = conn.Close()
	start := time.Now()
	if _, err := conn.Read(make([]byte, 1)); err == nil {
		t.Fatalf("expected error after close")
	}
	if time.Since(start) > 2*time.Second {
		t.Fatalf("close did not cancel the in-flight dial")
	}
}

func TestDialWithGraceConnectTimeout(t *testing.T) {
	d := newTestDialer(10*time.Second, nil)
	start := time.Now()
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 120*time.Millisecond, 20*time.Millisecond)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if _, err := conn.Read(make([]byte, 1)); err == nil {
		t.Fatalf("expected timeout error")
	}
	if elapsed := time.Since(start); elapsed > 2*time.Second {
		t.Fatalf("connect timeout not honoured, took %s", elapsed)
	}
	conn.Close()
}

// Regression: gonet.DialContextTCP returns a typed-nil conn together with the
// error; the deferred Close in go-socks5 must not panic on it.
func TestDialWithGraceTypedNilConnDoesNotPanic(t *testing.T) {
	d := dial.NewDialer(&fakeStack{delay: 150 * time.Millisecond, typedNilWithErr: true}, nil, nil, true, "")
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 5*time.Second, 20*time.Millisecond)
	if err != nil {
		t.Fatalf("expected deferred conn, got error: %v", err)
	}
	if _, err := conn.Read(make([]byte, 1)); err == nil {
		t.Fatalf("expected dial error")
	}
	conn.Close() // must not panic
}

// Regression: the SOCKS5 reply serialises LocalAddr(); a nil IP would produce a
// truncated reply and make the client hang until its own timeout.
func TestGracefulConnLocalAddrIsValidIPv4(t *testing.T) {
	d := newTestDialer(10*time.Second, nil)
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 30*time.Second, 20*time.Millisecond)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	addr, ok := conn.LocalAddr().(*net.TCPAddr)
	if !ok {
		t.Fatalf("unexpected LocalAddr type %T", conn.LocalAddr())
	}
	if addr.IP.To4() == nil || len(addr.IP.To4()) != net.IPv4len {
		t.Fatalf("LocalAddr must be a valid IPv4 for the SOCKS5 reply, got %v", addr.IP)
	}
	_ = conn.Close()
}

// flakyStack fails the first N handshake attempts, then succeeds.
type flakyStack struct {
	failFirst int
	attempts  int32
}

func (f *flakyStack) Run()                             {}
func (f *flakyStack) SetupResolve(_ zcdns.LocalServer) {}

func (f *flakyStack) DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error) {
	n := atomic.AddInt32(&f.attempts, 1)
	if int(n) <= f.failFirst {
		return nil, errors.New("syn lost")
	}
	client, server := net.Pipe()
	go func() {
		_, _ = server.Write([]byte{'x'})
	}()
	return client, nil
}

func (f *flakyStack) DialTCP(addr *net.TCPAddr) (net.Conn, error) {
	return f.DialTCPContext(context.Background(), addr)
}

func (f *flakyStack) DialUDP(addr *net.UDPAddr) (net.Conn, error) { return nil, errors.New("no udp") }

// A wedged handshake must be replaced by a fresh attempt (new gVisor endpoint /
// source port), which is what recovers from RVPN's short stalls.
func TestDialWithGraceRetriesWedgedHandshake(t *testing.T) {
	fs := &flakyStack{failFirst: 2}
	d := dial.NewDialer(fs, nil, nil, true, "")
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 5*time.Second, 20*time.Millisecond)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	buf := make([]byte, 1)
	if _, err := conn.Read(buf); err != nil {
		t.Fatalf("expected success after retries, got %v", err)
	}
	if got := atomic.LoadInt32(&fs.attempts); got != 3 {
		t.Fatalf("expected 3 attempts, got %d", got)
	}
	_ = conn.Close()
}

func TestDialWithGraceRetryBudgetIsRespected(t *testing.T) {
	fs := &flakyStack{failFirst: 1000}
	d := dial.NewDialer(fs, nil, nil, true, "")
	start := time.Now()
	conn, err := dialWithGrace(context.Background(), d, "tcp", "10.134.142.226:22", 700*time.Millisecond, 20*time.Millisecond)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if _, err := conn.Read(make([]byte, 1)); err == nil {
		t.Fatalf("expected failure")
	}
	if elapsed := time.Since(start); elapsed > 3*time.Second {
		t.Fatalf("retry budget not respected, took %s", elapsed)
	}
	_ = conn.Close()
}
