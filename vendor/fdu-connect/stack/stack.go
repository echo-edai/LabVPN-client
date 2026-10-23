package stack

import (
	"context"
	"net"

	"github.com/akirasalvare/fdu-connect/internal/zcdns"
)

type Stack interface {
	Run()
	SetupResolve(r zcdns.LocalServer)
	// DialTCPContext dials a TCP connection and aborts the handshake when ctx
	// is cancelled or its deadline is reached. Implementations backed by a
	// user-space stack must honour cancellation so that slow RVPN handshakes
	// do not leak goroutines.
	DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error)
	DialTCP(addr *net.TCPAddr) (net.Conn, error)
	DialUDP(addr *net.UDPAddr) (net.Conn, error)
}
