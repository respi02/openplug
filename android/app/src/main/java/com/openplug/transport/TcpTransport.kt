package com.openplug.transport

import java.net.InetSocketAddress
import java.net.Socket

/**
 * TCP transport for development: reaches `cargo run --example fake_dongle` on the
 * host through `adb reverse tcp:4455 tcp:4455`. Offered in debug builds only.
 */
class TcpTransport(private val host: String, private val port: Int) : StreamTransport() {

    private val socket = Socket()

    override fun open() {
        socket.tcpNoDelay = true
        socket.connect(InetSocketAddress(host, port), 5_000)
        input = socket.getInputStream()
        output = socket.getOutputStream()
    }

    override fun close() {
        runCatching { socket.close() }
    }
}
