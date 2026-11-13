package com.openplug.transport

import com.openplug.core.TransportException
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream

/** Shared send/recv over a pair of blocking streams. */
abstract class StreamTransport : DongleTransport {
    protected lateinit var input: InputStream
    protected lateinit var output: OutputStream

    override fun send(data: ByteArray) {
        try {
            output.write(data)
            output.flush()
        } catch (e: IOException) {
            throw TransportException.Io(e.message ?: "write failed")
        }
    }

    /** Blocks for at least one byte; an empty array means the connection is closed. */
    override fun recv(maxLen: UInt): ByteArray {
        val buf = ByteArray(maxLen.toInt())
        val n = try {
            input.read(buf)
        } catch (e: IOException) {
            -1 // Android reports a remote (or local) close as an IOException
        }
        return if (n <= 0) ByteArray(0) else buf.copyOf(n)
    }
}
