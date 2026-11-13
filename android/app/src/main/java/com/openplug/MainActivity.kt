package com.openplug

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.openplug.core.Reading
import com.openplug.transport.BluetoothSppTransport
import com.openplug.transport.DongleTransport
import com.openplug.transport.TcpTransport
import java.util.Locale

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()) {
                OpenPlugScreen()
            }
        }
    }
}

/** Something the user can connect to: a bonded dongle, or (debug) the fake one. */
private data class Target(val id: String, val label: String, val open: () -> DongleTransport)

private const val PREFS = "openplug"

private fun hasBluetoothPermission(context: Context): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
        PackageManager.PERMISSION_GRANTED

@SuppressLint("MissingPermission") // called only when hasBluetoothPermission() is true
private fun targets(context: Context): List<Target> {
    val bonded: Set<BluetoothDevice> =
        context.getSystemService(BluetoothManager::class.java)?.adapter?.bondedDevices.orEmpty()
    val dongles = bonded
        .sortedByDescending { it.name?.startsWith("DP_") == true } // DataPlugs first
        .map { d -> Target(d.address, d.name ?: d.address) { BluetoothSppTransport(d) } }
    val fake = Target("tcp", "Fake dongle (TCP 127.0.0.1:4455)") { TcpTransport("127.0.0.1", 4455) }
    return if (BuildConfig.DEBUG) dongles + fake else dongles
}

@Composable
fun OpenPlugScreen(vm: DongleViewModel = viewModel()) {
    val state by vm.state.collectAsStateWithLifecycle()
    val readings by vm.readings.collectAsStateWithLifecycle()

    Scaffold { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp)) {
            Text("OpenPlug", style = MaterialTheme.typography.headlineMedium)
            Spacer(Modifier.height(12.dp))
            when (state) {
                LinkState.Connected -> Live(readings, onDisconnect = vm::disconnect)
                else -> Setup(state, onConnect = vm::connect)
            }
        }
    }
}

@Composable
private fun Setup(state: LinkState, onConnect: (DongleTransport, String) -> Unit) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences(PREFS, Context.MODE_PRIVATE) }
    var granted by remember { mutableStateOf(hasBluetoothPermission(context)) }
    val askPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        granted = it
    }
    var pin by remember { mutableStateOf(prefs.getString("pin", "") ?: "") }
    var selected by remember { mutableStateOf(prefs.getString("target", null)) }
    val busy = state is LinkState.Connecting

    if (!granted) {
        Text("OpenPlug needs the Nearby devices permission to reach the dongle over Bluetooth.")
        Spacer(Modifier.height(8.dp))
        Button(onClick = { askPermission.launch(Manifest.permission.BLUETOOTH_CONNECT) }) {
            Text("Allow Bluetooth")
        }
        return
    }

    val all = remember(granted) { targets(context) }
    if (all.isEmpty()) {
        Text("No paired Bluetooth devices. Pair the dongle (DP_…) in the system Bluetooth settings first.")
        return
    }

    Text("Dongle", style = MaterialTheme.typography.titleMedium)
    all.forEach { t ->
        Row(
            Modifier.fillMaxWidth().selectable(selected = t.id == selected, enabled = !busy) { selected = t.id },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            RadioButton(selected = t.id == selected, onClick = null, modifier = Modifier.padding(12.dp))
            Text(t.label)
        }
    }
    Spacer(Modifier.height(8.dp))
    OutlinedTextField(
        value = pin,
        onValueChange = { pin = it.trim() },
        label = { Text("PIN from the dongle label") },
        singleLine = true,
        enabled = !busy,
        visualTransformation = PasswordVisualTransformation(),
        // label PINs are alphanumeric (ours is 8 capital letters)
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.Characters,
            keyboardType = KeyboardType.Password,
        ),
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(12.dp))
    val target = all.firstOrNull { it.id == selected }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Button(
            enabled = !busy && target != null && pin.isNotEmpty(),
            onClick = {
                prefs.edit().putString("pin", pin).putString("target", selected).apply()
                onConnect(target!!.open(), pin)
            },
        ) { Text("Connect") }
        if (busy) {
            CircularProgressIndicator(Modifier.padding(start = 16.dp))
        }
    }
    if (state is LinkState.Failed) {
        Spacer(Modifier.height(12.dp))
        Text(state.message, color = MaterialTheme.colorScheme.error)
    }
}

@Composable
private fun Live(readings: List<Reading>, onDisconnect: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("Connected", Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        OutlinedButton(onClick = onDisconnect) { Text("Disconnect") }
    }
    Spacer(Modifier.height(8.dp))
    if (readings.isEmpty()) {
        Text("Waiting for the first values…")
    }
    LazyColumn {
        items(readings) { r -> // no keys: a keyed list would scroll away from rows inserted above
            ListItem(
                headlineContent = { Text(r.name ?: String.format(Locale.ROOT, "0x%06x", r.url.toInt())) },
                trailingContent = { Text(formatValue(r), style = MaterialTheme.typography.titleMedium) },
            )
            HorizontalDivider()
        }
    }
}

private fun formatValue(r: Reading): String {
    if (!r.available) return "—"
    val number = String.format(Locale.getDefault(), "%.${r.precision.toInt()}f", r.value)
    return if (r.unit != null) "$number ${r.unit}" else number
}
