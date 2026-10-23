package gvisor

import (
	"context"
	"gvisor.dev/gvisor/pkg/tcpip"
	"gvisor.dev/gvisor/pkg/tcpip/adapters/gonet"
	"gvisor.dev/gvisor/pkg/tcpip/header"
	"net"
)

func (s *Stack) DialTCP(addr *net.TCPAddr) (net.Conn, error) {
	return s.DialTCPContext(context.Background(), addr)
}

// DialTCPContext uses gonet.DialContextTCP so a slow/black-holed RVPN
// handshake can be cancelled, and the caller can bound how long it waits.
func (s *Stack) DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error) {
	return gonet.DialContextTCP(ctx, s.gvisorStack, tcpip.FullAddress{
		NIC:  NICID,
		Port: uint16(addr.Port),
		Addr: tcpip.AddrFromSlice(addr.IP),
	}, header.IPv4ProtocolNumber)
}

func (s *Stack) DialUDP(addr *net.UDPAddr) (net.Conn, error) {
	return gonet.DialUDP(s.gvisorStack, nil, &tcpip.FullAddress{
		NIC:  NICID,
		Port: uint16(addr.Port),
		Addr: tcpip.AddrFromSlice(addr.IP),
	}, header.IPv4ProtocolNumber)
}
