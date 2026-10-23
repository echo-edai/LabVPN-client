package tun

import (
	"context"
	"net"
)

func (s *Stack) DialTCP(addr *net.TCPAddr) (net.Conn, error) {
	return s.DialTCPContext(context.Background(), addr)
}

func (s *Stack) DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error) {
	return s.endpoint.tcpDialer.DialContext(ctx, "tcp4", addr.String())
}

func (s *Stack) DialUDP(addr *net.UDPAddr) (net.Conn, error) {
	return s.endpoint.udpDialer.Dial("udp4", addr.String())
}
