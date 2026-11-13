package com.openplug.transport

import com.openplug.core.ByteTransport

/** A [ByteTransport] the app can open and tear down. */
interface DongleTransport : ByteTransport {
    /** Blocking; throws IOException (or SecurityException) when the link can't be opened. */
    fun open()

    /** Safe to call from any thread; unblocks a pending [recv]. */
    fun close()
}
