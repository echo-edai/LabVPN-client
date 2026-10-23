package tun

import (
	"context"
	"inet.af/netaddr"
	"net"
)

func (s *Stack) DialTCP(addr *net.TCPAddr) (net.Conn, error) {
	return s.DialTCPContext(context.Background(), addr)
}

func (s *Stack) DialTCPContext(ctx context.Context, addr *net.TCPAddr) (net.Conn, error) {
	prefix, ok := netaddr.FromStdIP(addr.IP)
	if ok && s.endpoint.ipSet.Contains(prefix) {
		return s.endpoint.tcpDialer.DialContext(ctx, "tcp4", addr.String())
	}
	return (&net.Dialer{}).DialContext(ctx, "tcp4", addr.String())
}

func (s *Stack) DialUDP(addr *net.UDPAddr) (net.Conn, error) {
	prefix, ok := netaddr.FromStdIP(addr.IP)
	if ok && s.endpoint.ipSet.Contains(prefix) {
		return s.endpoint.udpDialer.Dial("udp4", addr.String())
	}
	return net.DialUDP("udp4", nil, addr)
}
