package gvisor

import (
	"errors"
	"time"

	"github.com/akirasalvare/fdu-connect/client"
	"github.com/akirasalvare/fdu-connect/internal/hook_func"
	"github.com/akirasalvare/fdu-connect/internal/zcdns"
	"github.com/akirasalvare/fdu-connect/log"
	"gvisor.dev/gvisor/pkg/buffer"
	"gvisor.dev/gvisor/pkg/tcpip"
	"gvisor.dev/gvisor/pkg/tcpip/header"
	"gvisor.dev/gvisor/pkg/tcpip/network/ipv4"
	"gvisor.dev/gvisor/pkg/tcpip/stack"
	"gvisor.dev/gvisor/pkg/tcpip/transport/tcp"
	"gvisor.dev/gvisor/pkg/tcpip/transport/udp"
	"io"
)

type Stack struct {
	gvisorStack *stack.Stack
	resolve     zcdns.LocalServer

	endpoint *Endpoint
}

const NICID tcpip.NICID = 1
const MTU uint32 = 1400

type Endpoint struct {
	easyConnectClient *client.EasyConnectClient

	rvpnConn io.ReadWriteCloser

	dispatcher stack.NetworkDispatcher
}

func (ep *Endpoint) ParseHeader(*stack.PacketBuffer) bool {
	return true
}

func (ep *Endpoint) MTU() uint32 {
	return MTU
}

func (ep *Endpoint) SetMTU(mtu uint32) {
	log.Println("don't support change MTU from %d to %d", MTU, mtu)
}

func (ep *Endpoint) MaxHeaderLength() uint16 {
	return 0
}

func (ep *Endpoint) LinkAddress() tcpip.LinkAddress {
	return ""
}

func (ep *Endpoint) SetLinkAddress(addr tcpip.LinkAddress) {}

func (ep *Endpoint) Capabilities() stack.LinkEndpointCapabilities {
	return stack.CapabilityNone
}

func (ep *Endpoint) Attach(dispatcher stack.NetworkDispatcher) {
	ep.dispatcher = dispatcher
}

func (ep *Endpoint) IsAttached() bool {
	return ep.dispatcher != nil
}

func (ep *Endpoint) Wait() {}

func (ep *Endpoint) ARPHardwareType() header.ARPHardwareType {
	return header.ARPHardwareNone
}

func (ep *Endpoint) AddHeader(*stack.PacketBuffer) {}

func (ep *Endpoint) Close() {}

func (ep *Endpoint) SetOnCloseAction(func()) {}

// WritePackets is called when get packets from gVisor stack. Then it sends them to VPN server
func (ep *Endpoint) WritePackets(list stack.PacketBufferList) (int, tcpip.Error) {
	for _, packetBuffer := range list.AsSlice() {
		var buf []byte
		for _, t := range packetBuffer.AsSlices() {
			buf = append(buf, t...)
		}

		if ep.rvpnConn != nil {
			n, err := ep.rvpnConn.Write(buf)
			if err != nil {
				if hook_func.IsTerminal() {
					return list.Len(), nil
				} else {
					panic(err)
				}
			}
			log.DebugPrintf("Send: wrote %d bytes", n)
			log.DebugDumpHex(buf[:n])
		}
	}

	return list.Len(), nil
}

func NewStack(easyConnectClient *client.EasyConnectClient) (*Stack, error) {
	s := &Stack{}

	s.gvisorStack = stack.New(stack.Options{
		NetworkProtocols:   []stack.NetworkProtocolFactory{ipv4.NewProtocol},
		TransportProtocols: []stack.TransportProtocolFactory{tcp.NewProtocol, udp.NewProtocol},
		HandleLocal:        true,
	})

	s.endpoint = &Endpoint{
		easyConnectClient: easyConnectClient,
	}

	tcpipErr := s.gvisorStack.CreateNIC(NICID, s.endpoint)
	if tcpipErr != nil {
		return nil, errors.New(tcpipErr.String())
	}

	ip, err := easyConnectClient.IP()
	if err != nil {
		return nil, err
	}

	addr := tcpip.AddrFromSlice(ip)
	protoAddr := tcpip.ProtocolAddress{
		AddressWithPrefix: tcpip.AddressWithPrefix{
			Address:   addr,
			PrefixLen: 32,
		},
		Protocol: ipv4.ProtocolNumber,
	}

	tcpipErr = s.gvisorStack.AddProtocolAddress(NICID, protoAddr, stack.AddressProperties{})
	if tcpipErr != nil {
		return nil, errors.New(tcpipErr.String())
	}

	sOpt := tcpip.TCPSACKEnabled(true)
	s.gvisorStack.SetTransportProtocolOption(tcp.ProtocolNumber, &sOpt)
	cOpt := tcpip.CongestionControlOption("cubic")
	s.gvisorStack.SetTransportProtocolOption(tcp.ProtocolNumber, &cOpt)

	// 校园网/RVPN 路径上目标主机偶发丢包时，gVisor 默认 1s 起步的 SYN 重传会让
	// 首次握手在数秒内几乎只有 2~3 次机会。把 RTO 下限降到 500ms（仍远大于
	// 校内 RTT 的十几毫秒），让 15s 的建连预算内能重试更多次，显著提高
	// 「路径短暂抖动」时的成功率。
	rtoOpt := tcpip.TCPMinRTOOption(500 * time.Millisecond)
	if rtoErr := s.gvisorStack.SetTransportProtocolOption(tcp.ProtocolNumber, &rtoOpt); rtoErr != nil {
		log.Printf("Failed to set TCP min RTO: %v", rtoErr)
	}
	s.gvisorStack.AddRoute(tcpip.Route{Destination: header.IPv4EmptySubnet, NIC: NICID})

	return s, nil
}

func (s *Stack) SetupResolve(r zcdns.LocalServer) {
	s.resolve = r
}

func (s *Stack) Run() {
	var connErr error
	s.endpoint.rvpnConn, connErr = client.NewRvpnConn(s.endpoint.easyConnectClient)
	if connErr != nil {
		panic(connErr)
	}
	// Read from VPN server and send to gVisor stack
	for {
		buf := make([]byte, MTU)
		n, err := s.endpoint.rvpnConn.Read(buf)
		if err != nil {
			if hook_func.IsTerminal() {
				return
			} else {
				panic(err)
			}
		}
		log.DebugPrintf("Recv: read %d bytes", n)
		log.DebugDumpHex(buf[:n])

		packetBuffer := stack.NewPacketBuffer(stack.PacketBufferOptions{
			Payload: buffer.MakeWithData(buf),
		})
		s.endpoint.dispatcher.DeliverNetworkPacket(header.IPv4ProtocolNumber, packetBuffer)
		packetBuffer.DecRef()
	}
}
