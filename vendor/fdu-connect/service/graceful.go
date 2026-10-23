package service

import (
	"context"
	"net"
	"sync"
	"time"

	"github.com/akirasalvare/fdu-connect/dial"
	"github.com/akirasalvare/fdu-connect/log"
)

// gracefulConn is a net.Conn proxy that is handed back to the SOCKS5 server
// before the upstream connection is fully established.
//
// Why: the RVPN tunnel builds a full TCP handshake in a user-space gVisor
// stack. When an intranet host (or the path towards it) is temporarily lossy,
// that handshake can take far longer than the SOCKS5 client's own dial
// timeout (mihomo gives up after 5s), so every such connection is reported as
// "context deadline exceeded" and never gets a chance to succeed even though
// the tunnel would have established it a few seconds later.
//
// Read/Write block until the real connection is ready, so no data is lost or
// reordered; Close cancels the in-flight dial (gVisor's DialContextTCP honours
// the context, so no goroutine is leaked).
type gracefulConn struct {
	ctx     context.Context
	cancel  context.CancelFunc
	dialer  *dial.Dialer
	network string
	addr    string
	ready   chan struct{}

	mu            sync.Mutex
	conn          net.Conn
	err           error
	closed        bool
	readDeadline  time.Time
	writeDeadline time.Time

	// connectTimeout is the total budget for all connection attempts (0 = no
	// limit and a single attempt). perAttempt bounds each individual attempt so
	// a wedged handshake can be replaced by a fresh one (new source port and a
	// new server-side NAT entry), which is what actually recovers from the
	// short RVPN stalls seen in the logs.
	connectTimeout time.Duration
	perAttempt     time.Duration
}

func newGracefulConn(ctx context.Context, d *dial.Dialer, network, addr string, connectTimeout time.Duration) *gracefulConn {
	ctx, cancel := context.WithCancel(ctx)
	perAttempt := time.Duration(0)
	if connectTimeout > 0 {
		perAttempt = connectAttemptTimeout
		if connectTimeout < perAttempt {
			perAttempt = connectTimeout
		}
	}
	c := &gracefulConn{
		ctx:            ctx,
		cancel:         cancel,
		dialer:         d,
		network:        network,
		addr:           addr,
		ready:          make(chan struct{}),
		connectTimeout: connectTimeout,
		perAttempt:     perAttempt,
	}
	go c.run()
	return c
}

// connectAttemptTimeout bounds one gVisor handshake attempt. It has to be well
// below the total budget so a wedged attempt can be retried with a fresh one.
const connectAttemptTimeout = 4 * time.Second

func (c *gracefulConn) run() {
	var conn net.Conn
	var err error
	deadline := time.Time{}
	if c.connectTimeout > 0 {
		deadline = time.Now().Add(c.connectTimeout)
	}
	for attempt := 1; ; attempt++ {
		attemptCtx, cancel := c.ctx, context.CancelFunc(func() {})
		if c.perAttempt > 0 {
			timeout := c.perAttempt
			if remain := time.Until(deadline); remain < timeout {
				timeout = remain
			}
			if timeout <= 0 {
				break
			}
			attemptCtx, cancel = context.WithTimeout(c.ctx, timeout)
		}
		conn, err = c.dialer.DialIPPort(attemptCtx, c.network, c.addr)
		cancel()
		if err == nil {
			break
		}
		if c.ctx.Err() != nil {
			break
		}
		if !deadline.IsZero() && !time.Now().Before(deadline) {
			break
		}
		log.Printf("SOCKS5: %s -> %s attempt %d failed (%v), retrying", c.network, c.addr, attempt, err)
		select {
		case <-time.After(200 * time.Millisecond):
		case <-c.ctx.Done():
		}
	}
	if err != nil {
		// gonet.DialContextTCP returns (nil *TCPConn, err); converting that to
		// net.Conn yields a non-nil interface holding a nil pointer, which
		// would panic on Close. Normalise it before publishing the result.
		conn = nil
	}
	c.mu.Lock()
	c.conn, c.err = conn, err
	closed := c.closed
	if conn != nil {
		if !c.readDeadline.IsZero() {
			_ = conn.SetReadDeadline(c.readDeadline)
		}
		if !c.writeDeadline.IsZero() {
			_ = conn.SetWriteDeadline(c.writeDeadline)
		}
	}
	c.mu.Unlock()
	if closed && conn != nil {
		_ = conn.Close()
	}
	close(c.ready)
}

// wait blocks until the upstream dial finishes and returns its result.
func (c *gracefulConn) wait() (net.Conn, error) {
	<-c.ready
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.conn, c.err
}

func (c *gracefulConn) Read(p []byte) (int, error) {
	conn, err := c.wait()
	if err != nil {
		return 0, err
	}
	return conn.Read(p)
}

func (c *gracefulConn) Write(p []byte) (int, error) {
	conn, err := c.wait()
	if err != nil {
		return 0, err
	}
	return conn.Write(p)
}

func (c *gracefulConn) Close() error {
	c.mu.Lock()
	if c.closed {
		c.mu.Unlock()
		return nil
	}
	c.closed = true
	c.mu.Unlock()

	// Cancel the in-flight dial (no-op once it finished) and close the real
	// connection if it is already available.
	c.cancel()
	c.mu.Lock()
	conn := c.conn
	c.mu.Unlock()
	if conn != nil {
		return conn.Close()
	}
	return nil
}

func (c *gracefulConn) LocalAddr() net.Addr {
	c.mu.Lock()
	conn := c.conn
	c.mu.Unlock()
	if conn != nil {
		return conn.LocalAddr()
	}
	// Must be a valid IPv4 (not the zero-value TCPAddr with a nil IP):
	// go-socks5 serialises it into the SOCKS5 reply, and a nil IP would make
	// the reply 4 bytes short, so clients would block waiting for the rest
	// until their own dial timeout fires.
	return &net.TCPAddr{IP: net.IPv4zero}
}

func (c *gracefulConn) RemoteAddr() net.Addr {
	return &net.TCPAddr{IP: net.IPv4zero}
}

func (c *gracefulConn) SetDeadline(t time.Time) error {
	return c.SetReadDeadline(t)
}

func (c *gracefulConn) SetReadDeadline(t time.Time) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.readDeadline = t
	if c.conn != nil {
		return c.conn.SetReadDeadline(t)
	}
	return nil
}

func (c *gracefulConn) SetWriteDeadline(t time.Time) error {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.writeDeadline = t
	if c.conn != nil {
		return c.conn.SetWriteDeadline(t)
	}
	return nil
}

// dialWithGrace starts dialing and either returns the real net.Conn (fast
// path, identical to the old behaviour) or, when the handshake is slower than
// `earlyReply`, a gracefulConn that keeps dialing in the background and blocks
// on first use. This keeps the SOCKS5 reply inside the client's dial timeout
// while still letting the tunnel take its time.
func dialWithGrace(ctx context.Context, d *dial.Dialer, network, addr string, connectTimeout, earlyReply time.Duration) (net.Conn, error) {
	if connectTimeout <= 0 && earlyReply <= 0 {
		return d.DialIPPort(ctx, network, addr)
	}

	c := newGracefulConn(ctx, d, network, addr, connectTimeout)
	if earlyReply <= 0 {
		conn, err := c.wait()
		c.cancel()
		if err != nil {
			_ = c.Close()
			return nil, err
		}
		return conn, nil
	}

	timer := time.NewTimer(earlyReply)
	defer timer.Stop()
	select {
	case <-c.ready:
		conn, err := c.wait()
		c.cancel()
		if err != nil {
			_ = c.Close()
			return nil, err
		}
		return conn, nil
	case <-timer.C:
		log.Printf(
			"SOCKS5: %s -> %s still connecting after %s, replying and waiting up to %s",
			network, addr, earlyReply, connectTimeout,
		)
		return c, nil
	}
}
