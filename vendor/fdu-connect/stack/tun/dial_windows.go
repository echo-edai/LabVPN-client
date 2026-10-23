package tun

import (
	"context"
	"net"
)

func (s *Stack) DialTCP(addr *net.TCPAddr) (net.Conn, error) {
	return s.DialTCPContext(context.Background(), addr)
}

func (s *Stack) DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error) {
	return (&net.Dialer{LocalAddr: &net.TCPAddr{IP: s.endpoint.ip}}).DialContext(ctx, "tcp4", addr.String())
}

func (s *Stack) DialUDP(addr *net.UDPAddr) (net.Conn, error) {
	return net.DialUDP("udp4", &net.UDPAddr{
		IP:   s.endpoint.ip,
		Port: 0,
	}, addr)
}
