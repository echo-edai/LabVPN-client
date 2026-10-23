package service

import (
	"context"
	"net"
	"time"

	"github.com/akirasalvare/fdu-connect/dial"
	"github.com/akirasalvare/fdu-connect/log"
	"github.com/akirasalvare/fdu-connect/resolve"
	"github.com/things-go/go-socks5"
)

// ServeSocks5 starts the SOCKS5 server.
//
// connectTimeout bounds how long the RVPN tunnel may take to establish a
// connection (0 disables the bound), and earlyReply is how long the server
// waits before answering the SOCKS5 client optimistically (0 disables the
// optimisation and restores the strict SOCKS5 behaviour).
func ServeSocks5(bindAddr string, dialer *dial.Dialer, resolver *resolve.Resolver, user string, password string, connectTimeout, earlyReply time.Duration) {
	var authMethods []socks5.Authenticator
	if user != "" && password != "" {
		authMethods = append(authMethods, socks5.UserPassAuthenticator{
			Credentials: socks5.StaticCredentials{user: password},
		})

		log.Println("Neither traffic nor credentials are encrypted in the SOCKS5 protocol!")
		log.Println("DO NOT deploy it to the public network. All consequences and responsibilities have nothing to do with the developer")
	} else {
		authMethods = append(authMethods, socks5.NoAuthAuthenticator{})
	}

	server := socks5.NewServer(
		socks5.WithAuthMethods(authMethods),
		socks5.WithResolver(resolver),
		socks5.WithDial(func(ctx context.Context, network, addr string) (net.Conn, error) {
			return dialWithGrace(ctx, dialer, network, addr, connectTimeout, earlyReply)
		}),
		socks5.WithLogger(socks5.NewLogger(log.NewLogger("[SOCKS5] "))),
	)

	log.Printf("SOCKS5 server listening on " + bindAddr)

	if err := server.ListenAndServe("tcp", bindAddr); err != nil {
		panic("SOCKS5 listen failed: " + err.Error())
	}
}
