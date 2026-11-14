package com.openplug

import android.text.format.DateUtils
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.openplug.core.RefuelRecord
import com.openplug.core.TripRecord
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.TextStyle
import java.util.Locale

/** A row of the log: a trip (finished or in progress) or a refuel. */
private sealed interface Entry {
    val timeMs: Long

    class TripEntry(val trip: TripRecord, val running: Boolean) : Entry {
        override val timeMs get() = trip.startMs.toLong()
    }

    class RefuelEntry(val refuel: RefuelRecord) : Entry {
        override val timeMs get() = refuel.timeMs.toLong()
    }
}

private fun dayOf(ms: Long): LocalDate = Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault()).toLocalDate()

@Composable
fun TripsTab() {
    val journal by Session.journal.collectAsStateWithLifecycle()
    val problem by Session.journalProblem.collectAsStateWithLifecycle()
    var period by rememberSaveable { mutableIntStateOf(1) }
    var openTrip by rememberSaveable { mutableStateOf<Long?>(null) } // start time of the trip shown in detail

    openTrip?.let { start ->
        val trip = journal.active?.takeIf { it.startMs.toLong() == start }
            ?: journal.trips.firstOrNull { it.startMs.toLong() == start }
        if (trip != null) {
            BackHandler { openTrip = null }
            TripDetail(trip, onBack = { openTrip = null })
            return
        }
    }

    val today = LocalDate.now()
    val from: LocalDate = when (period) {
        0 -> today.minusDays(6)
        1 -> today.withDayOfMonth(1)
        else -> today.withDayOfYear(1)
    }
    val periods = listOf(
        "7 days",
        today.month.getDisplayName(TextStyle.FULL_STANDALONE, Locale.getDefault()).replaceFirstChar(Char::titlecase),
        today.year.toString(),
    )

    val trips = journal.trips.filter { !dayOf(it.startMs.toLong()).isBefore(from) }
    val entries: List<Entry> = buildList {
        journal.active?.let { add(Entry.TripEntry(it, running = true)) }
        trips.forEach { add(Entry.TripEntry(it, running = false)) }
        journal.refuels.filter { !dayOf(it.timeMs.toLong()).isBefore(from) }.forEach { add(Entry.RefuelEntry(it)) }
    }.sortedByDescending { it.timeMs }
    val days = entries.groupBy { dayOf(it.timeMs) }.toList()

    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(horizontal = 20.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        item { ScreenTitle("Trips") }
        problem?.let {
            item { Text("Trips are not being recorded: $it", color = MaterialTheme.colorScheme.error) }
        }
        item {
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                periods.forEachIndexed { i, label ->
                    SegmentedButton(
                        selected = period == i,
                        onClick = { period = i },
                        shape = SegmentedButtonDefaults.itemShape(i, periods.size),
                        icon = {},
                    ) { Text(label) }
                }
            }
        }
        item { Summary(trips) }
        if (entries.isEmpty()) {
            item {
                Text(
                    "No trips in this period. A trip starts when the car moves while OpenPlug is connected, and ends 15 minutes after the connection drops.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        days.forEach { (day, rows) ->
            item(key = day.toEpochDay()) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Caption(dayLabel(day, today).uppercase())
                    Panel {
                        rows.forEachIndexed { i, entry ->
                            if (i > 0) HorizontalDivider(Modifier.padding(vertical = 12.dp), color = MaterialTheme.colorScheme.outlineVariant)
                            when (entry) {
                                is Entry.TripEntry -> TripRow(entry.trip, entry.running) {
                                    openTrip = entry.trip.startMs.toLong()
                                }
                                is Entry.RefuelEntry -> RefuelRow(entry.refuel)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun dayLabel(day: LocalDate, today: LocalDate): String = when (day) {
    today -> "Today"
    today.minusDays(1) -> "Yesterday"
    else -> DateUtils.formatDateTime(
        LocalContext.current,
        day.atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli(),
        DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_WEEKDAY or DateUtils.FORMAT_ABBREV_ALL,
    )
}

@Composable
private fun Summary(trips: List<TripRecord>) {
    val distance = trips.sumOf { it.distanceKm }
    val moving = trips.sumOf { it.movingS.toLong() }.toULong()
    val withFuel = trips.filter { it.fuelUsedL != null }
    val fuel = withFuel.sumOf { it.fuelUsedL ?: 0.0 }
    val fuelDistance = withFuel.sumOf { it.distanceKm }
    Panel {
        Row {
            Stat(km(distance), if (trips.size == 1) "1 trip" else "${trips.size} trips", Modifier.weight(1f))
            Stat(duration(moving), "driving", Modifier.weight(1f))
        }
        Row(Modifier.padding(top = 14.dp)) {
            Stat(if (fuel > 0) litres(fuel) else "–", "fuel used", Modifier.weight(1f))
            Stat(
                if (fuel > 0 && fuelDistance >= 1) number(fuel / fuelDistance * 100, 1) else "–",
                "l/100 km average",
                Modifier.weight(1f),
            )
        }
    }
}

@Composable
private fun Stat(value: String, label: String, modifier: Modifier) {
    Column(modifier) {
        Text(value, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
        Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun time(ms: ULong): String = DateUtils.formatDateTime(LocalContext.current, ms.toLong(), DateUtils.FORMAT_SHOW_TIME)

@Composable
private fun TripRow(trip: TripRecord, running: Boolean, onOpen: () -> Unit) {
    Row(Modifier.clickable(onClick = onOpen), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(
                if (running) time(trip.startMs) + " – now" else time(trip.startMs) + " – " + time(trip.endMs),
                fontWeight = FontWeight.SemiBold,
            )
            val details = buildList {
                add(duration(trip.movingS))
                trip.consumptionLPer100km?.let { add(number(it, 1) + " l/100 km") }
                if (running) add("in progress")
            }
            Text(
                details.joinToString(" · "),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Text(km(trip.distanceKm), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
    }
}

@Composable
private fun RefuelRow(refuel: RefuelRecord) {
    val colors = MaterialTheme.colorScheme
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier.size(32.dp).background(colors.tertiaryContainer, RoundedCornerShape(10.dp)),
            contentAlignment = Alignment.Center,
        ) { Icon(painterResource(R.drawable.ic_fuel), null, Modifier.size(18.dp), tint = colors.onTertiaryContainer) }
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Text("Refuel", fontWeight = FontWeight.SemiBold)
            val at = refuel.odometerKm?.let { " · at ${number(it, 0)} km" } ?: ""
            Text(time(refuel.timeMs) + at, style = MaterialTheme.typography.bodyMedium, color = colors.onSurfaceVariant)
        }
        Text("+" + litres(refuel.addedL), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold, color = colors.tertiary)
    }
}
