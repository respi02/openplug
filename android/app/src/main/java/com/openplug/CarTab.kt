package com.openplug

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.openplug.core.ServiceDue
import com.openplug.core.TripRecord

/** Data objects the Car tab shows (see the catalog in research/notes/live_protocol.md). */
private object Url {
    const val SPEED = 0x020000u
    const val FUEL_PERCENT = 0x020001u
    const val FUEL = 0x020002u
    const val ODOMETER = 0x020004u
    const val RANGE = 0x020005u
    const val OUTSIDE = 0x020006u
    const val OIL_DASH = 0x020030u
    const val RPM = 0x040000u
    const val COOLANT = 0x040001u
    const val OIL = 0x04000fu
    const val BATTERY = 0x050001u
    const val SERVICE_DAYS = 0x070009u
    const val SERVICE_KM = 0x070012u
}

/** A value to show: live while connected, otherwise the last one remembered. */
private class Shown(val value: Double, val precision: Int, val timeMs: Long?)

@Composable
fun CarTab(onSetUp: () -> Unit) {
    val state by Session.state.collectAsStateWithLifecycle()
    val readings by Session.readings.collectAsStateWithLifecycle()
    val journal by Session.journal.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val connected = state is LinkState.Connected

    val live = if (connected) readings.filter { it.available }.associateBy { it.url } else emptyMap()
    val last = journal.last.associateBy { it.url }
    fun shown(url: UInt): Shown? =
        live[url]?.let { Shown(it.value.toDouble(), it.precision.toInt(), null) }
            ?: last[url]?.let { Shown(it.value.toDouble(), it.precision.toInt(), it.timeMs.toLong()) }
    fun text(url: UInt, unit: String, decimals: Int? = null): String? =
        shown(url)?.let { number(it.value, decimals ?: it.precision) + unit }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            ScreenTitle("My car", Modifier.weight(1f))
            StatusPill(state)
        }

        val setUp = Settings.saved(context) != null
        when {
            state is LinkState.Failed -> Panel {
                Text((state as LinkState.Failed).message, color = MaterialTheme.colorScheme.error)
                Spacer(Modifier.height(8.dp))
                Button(onClick = onSetUp) { Text("Open settings") }
            }
            !setUp && state is LinkState.Idle -> Panel {
                Text("Connect your dongle", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(4.dp))
                Text(
                    "Pair the DataPlug in the system Bluetooth settings, then choose it and enter the PIN from its label.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(12.dp))
                Button(onClick = onSetUp) { Text("Set up") }
            }
            !connected -> {
                val seen = journal.last.maxOfOrNull { it.timeMs.toLong() }
                val lastSeen = seen?.let {
                    "Last seen " + DateUtils.getRelativeTimeSpanString(
                        it, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS,
                    ).toString().replaceFirstChar(Char::lowercase) + ". "
                } ?: ""
                val hint = when (state) {
                    is LinkState.Idle -> "Connect in Settings to start recording."
                    else -> "OpenPlug connects by itself when you switch the ignition on."
                }
                Text(lastSeen + hint, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }

        if (connected) {
            Hero(
                speed = text(Url.SPEED, "", 0) ?: "–",
                rpm = text(Url.RPM, "", 0) ?: "–",
                trip = journal.active.takeIf { journal.tripRunning },
            )
        }

        journal.service?.takeIf { it.soon }?.let { ServiceDueCard(it) }

        val fuel = shown(Url.FUEL)
        val odometer = text(Url.ODOMETER, "", 0)
        if (fuel != null || odometer != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Panel(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            painterResource(R.drawable.ic_fuel), null,
                            Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Caption("Fuel", Modifier.padding(start = 8.dp))
                    }
                    BigValue(fuel?.let { number(it.value, 1) + " l" } ?: "–")
                    (shown(Url.FUEL_PERCENT)?.let { it.value / 100 } ?: journal.fuelFraction)?.let { level ->
                        val low = level < 0.15
                        LinearProgressIndicator(
                            progress = { level.toFloat().coerceIn(0f, 1f) },
                            modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
                            color = if (low) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.primary,
                            drawStopIndicator = {},
                        )
                    }
                    text(Url.RANGE, " km range", 0)?.let { Caption(it) }
                }
                Panel(Modifier.weight(1f)) {
                    Caption("Odometer")
                    BigValue(odometer ?: "–")
                    Caption("km")
                }
            }
        }

        if (!connected) {
            if (Settings.recordRoutes(context)) journal.parked?.let { ParkedCard(it) }
            (journal.active ?: journal.trips.firstOrNull())?.let { LastTrip(it) }
            val days = text(Url.SERVICE_DAYS, " days", 0)
            val distance = text(Url.SERVICE_KM, " km", 0)
            val due = journal.service
            if (due != null && !due.soon) {
                Panel {
                    Caption("Next service")
                    Text(serviceLeft(due), Modifier.padding(top = 8.dp), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                }
            } else if (due == null && (days != null || distance != null)) {
                Panel {
                    Caption("Since the last service")
                    Spacer(Modifier.height(8.dp))
                    Row {
                        days?.let { Text(it, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold) }
                        distance?.let { Text(it, Modifier.weight(1f), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold) }
                    }
                }
            }
        }

        val rows = buildList {
            if (connected) {
                text(Url.COOLANT, " °C", 0)?.let { add("Coolant" to it) }
                (text(Url.OIL, " °C", 0) ?: text(Url.OIL_DASH, " °C", 0))?.let { add("Oil" to it) }
            }
            text(Url.BATTERY, " V", 1)?.let { add((if (connected) "Battery" else "Battery when last seen") to it) }
            if (connected) text(Url.OUTSIDE, " °C", 0)?.let { add("Outside" to it) }
        }
        if (rows.isNotEmpty()) {
            Panel {
                rows.forEachIndexed { i, (label, value) ->
                    if (i > 0) HorizontalDivider(Modifier.padding(vertical = 10.dp), color = MaterialTheme.colorScheme.outlineVariant)
                    Row {
                        Text(label, Modifier.weight(1f))
                        Text(value, fontWeight = FontWeight.SemiBold)
                    }
                }
            }
        }
    }
}

/** "in 580 km or in 25 days" — what is left until the next service. */
private fun serviceLeft(due: ServiceDue): String = buildList {
    due.kmLeft?.let { add(if (it < 0) number(-it, 0) + " km overdue" else "in " + number(it, 0) + " km") }
    due.daysLeft?.let {
        add(
            when {
                it < 0 -> "${-it} days overdue"
                it == 0L -> "today"
                it == 1L -> "tomorrow"
                else -> "in $it days"
            },
        )
    }
}.joinToString(" or ")

/** A reminder that the service is near or overdue. */
@Composable
private fun ServiceDueCard(due: ServiceDue) {
    val colors = MaterialTheme.colorScheme
    val overdue = (due.kmLeft ?: 1.0) < 0 || (due.daysLeft ?: 1L) < 0
    Surface(
        Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = colors.surfaceContainerLowest,
        border = androidx.compose.foundation.BorderStroke(1.dp, colors.tertiary.copy(alpha = 0.5f)),
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier.size(32.dp).background(colors.tertiaryContainer, RoundedCornerShape(10.dp)),
                contentAlignment = Alignment.Center,
            ) { Text("!", fontWeight = FontWeight.Bold, color = colors.onTertiaryContainer) }
            Column(Modifier.padding(start = 12.dp)) {
                Text(if (overdue) "Service overdue" else "Service due soon", fontWeight = FontWeight.SemiBold)
                Text(serviceLeft(due), style = MaterialTheme.typography.bodyMedium, color = colors.onSurfaceVariant)
            }
        }
    }
}

@Composable
private fun StatusPill(state: LinkState) {
    val (label, active) = when (state) {
        is LinkState.Connected -> "Connected · recording" to true
        is LinkState.Connecting -> "Connecting…" to false
        is LinkState.Waiting -> "Waiting for the car" to false
        is LinkState.Failed -> "Stopped" to false
        LinkState.Idle -> "Not connected" to false
    }
    val colors = MaterialTheme.colorScheme
    Surface(
        shape = RoundedCornerShape(50),
        color = if (active) colors.primaryContainer else colors.surfaceContainerHighest,
        contentColor = if (active) colors.onPrimaryContainer else colors.onSurfaceVariant,
    ) {
        Row(Modifier.padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(8.dp).background(if (active) colors.primary else colors.onSurfaceVariant, CircleShape))
            Spacer(Modifier.width(8.dp))
            Text(label, style = MaterialTheme.typography.labelLarge)
        }
    }
}

/** The dark card shown while connected: speed, engine speed and the trip in progress. */
@Composable
private fun Hero(speed: String, rpm: String, trip: TripRecord?) {
    val colors = MaterialTheme.colorScheme
    val quiet = colors.inverseOnSurface.copy(alpha = 0.7f)
    Surface(Modifier.fillMaxWidth(), shape = RoundedCornerShape(22.dp), color = colors.inverseSurface, contentColor = colors.inverseOnSurface) {
        Column(Modifier.padding(20.dp)) {
            Row {
                Text(if (trip != null) "CURRENT TRIP" else "LIVE", Modifier.weight(1f), style = MaterialTheme.typography.labelLarge, color = quiet)
                trip?.let {
                    Text(
                        "since " + DateUtils.formatDateTime(LocalContext.current, it.startMs.toLong(), DateUtils.FORMAT_SHOW_TIME),
                        style = MaterialTheme.typography.labelLarge, color = quiet,
                    )
                }
            }
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.Bottom) {
                Text(speed, fontSize = 72.sp, lineHeight = 72.sp, fontWeight = FontWeight.SemiBold)
                Text(" km/h", Modifier.weight(1f).padding(bottom = 10.dp), color = quiet)
                Column(horizontalAlignment = Alignment.End, modifier = Modifier.padding(bottom = 6.dp)) {
                    Text(rpm, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
                    Text("rpm", style = MaterialTheme.typography.bodySmall, color = quiet)
                }
            }
            if (trip != null) {
                HorizontalDivider(Modifier.padding(vertical = 14.dp), color = Color.White.copy(alpha = 0.15f))
                Row {
                    HeroStat(km(trip.distanceKm).removeSuffix(" km") + " km", "distance", quiet, Modifier.weight(1f))
                    HeroStat(duration(trip.movingS), "driving", quiet, Modifier.weight(1f))
                    val consumption = trip.consumptionLPer100km
                    HeroStat(consumption?.let { number(it, 1) + " l" } ?: "–", "per 100 km", quiet, Modifier.weight(1f))
                }
            }
        }
    }
}

@Composable
private fun HeroStat(value: String, label: String, quiet: Color, modifier: Modifier) {
    Column(modifier) {
        Text(value, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
        Text(label, style = MaterialTheme.typography.bodySmall, color = quiet)
    }
}

@Composable
private fun BigValue(text: String) {
    Text(text, Modifier.padding(top = 8.dp), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
}

@Composable
private fun LastTrip(trip: TripRecord) {
    val context = LocalContext.current
    fun time(ms: ULong) = DateUtils.formatDateTime(context, ms.toLong(), DateUtils.FORMAT_SHOW_TIME)
    Panel {
        Row {
            Caption("Last trip", Modifier.weight(1f))
            Caption(
                DateUtils.getRelativeTimeSpanString(
                    trip.startMs.toLong(), System.currentTimeMillis(), DateUtils.DAY_IN_MILLIS,
                ).toString() + " · " + time(trip.startMs) + " – " + time(trip.endMs),
            )
        }
        Row(Modifier.padding(top = 8.dp), verticalAlignment = Alignment.Bottom) {
            Text(km(trip.distanceKm), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
            val details = buildList {
                add(duration(trip.movingS))
                trip.consumptionLPer100km?.let { add(number(it, 1) + " l/100 km") }
            }
            Text(
                details.joinToString(" · "),
                Modifier.padding(start = 14.dp, bottom = 4.dp),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
