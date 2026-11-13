package com.openplug

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * Minimal Compose UI skeleton: connection state + a list of live data objects.
 * The real views (trips, fuel, statistics) come later.
 *
 * NOTE: scaffold — not built here. A `DongleViewModel` would own the UniFFI
 * `Dongle` and run this loop on a background dispatcher:
 *
 *     val transport = BluetoothSppTransport(device).apply { open() }
 *     val dongle = Dongle.connect(transport, pin)   // throws DongleException.WrongPin
 *     dongle.subscribeDefaults()
 *     while (isActive) publish(dongle.nextReadings())
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MaterialTheme { OpenPlugScreen() } }
    }
}

data class Reading(val name: String, val value: String)

@Composable
fun OpenPlugScreen() {
    var connected by remember { mutableStateOf(false) }
    val readings = remember { mutableStateListOf<Reading>() }

    Column(Modifier.fillMaxSize().padding(16.dp)) {
        Text("OpenPlug", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(8.dp))
        Text(if (connected) "Connected" else "Disconnected")
        Spacer(Modifier.height(12.dp))
        Button(onClick = { /* TODO: start the DongleViewModel loop above */ }) {
            Text(if (connected) "Disconnect" else "Connect DataPlug")
        }
        Spacer(Modifier.height(16.dp))
        LazyColumn {
            items(readings) { r ->
                ListItem(
                    headlineContent = { Text(r.name) },
                    trailingContent = { Text(r.value) },
                )
            }
        }
    }
}
