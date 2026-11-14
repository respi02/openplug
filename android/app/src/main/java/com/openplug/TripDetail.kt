package com.openplug

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.text.format.DateUtils
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.openplug.core.ParkedAt
import com.openplug.core.RoutePoint
import com.openplug.core.TripRecord
import org.osmdroid.config.Configuration
import org.osmdroid.tileprovider.tilesource.TileSourceFactory
import org.osmdroid.util.BoundingBox
import org.osmdroid.util.GeoPoint
import org.osmdroid.views.CustomZoomButtonsController
import org.osmdroid.views.MapView
import org.osmdroid.views.overlay.Marker
import org.osmdroid.views.overlay.Polyline

/** One trip: its route on a map (when positions were recorded), the speed along it, its numbers. */
@Composable
fun TripDetail(trip: TripRecord, onBack: () -> Unit) {
    val context = LocalContext.current
    val journal by Session.journal.collectAsStateWithLifecycle()
    // a trip in progress grows: reload its route as the journal moves on
    val live = journal.active?.takeIf { it.startMs == trip.startMs }
    val shown = live ?: trip
    val route = remember(trip.startMs, live?.movingS) { Session.route(trip.startMs) }
    fun time(ms: ULong) = DateUtils.formatDateTime(context, ms.toLong(), DateUtils.FORMAT_SHOW_TIME)

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        TextButton(onClick = onBack, contentPadding = PaddingValues(0.dp)) { Text("‹ Trips") }
        Column {
            Text(
                DateUtils.formatDateTime(
                    context, shown.startMs.toLong(),
                    DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_WEEKDAY or DateUtils.FORMAT_ABBREV_ALL,
                ) + " · " + time(shown.startMs) + " – " + time(shown.endMs),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(km(shown.distanceKm), style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.SemiBold)
        }

        if (route.size >= 2) {
            RouteMap(route, Modifier.fillMaxWidth().height(260.dp).clip(RoundedCornerShape(18.dp)))
            Panel {
                Row {
                    Caption("Speed", Modifier.weight(1f))
                    Caption("max ${number(shown.maxSpeedKmh.toDouble(), 0)} km/h")
                }
                SpeedChart(route, Modifier.fillMaxWidth().height(96.dp).padding(top = 10.dp))
                Row(Modifier.padding(top = 4.dp)) {
                    Caption(time(route.first().timeMs), Modifier.weight(1f))
                    Caption(time(route.last().timeMs))
                }
            }
        } else {
            Text(
                "No route was recorded for this trip. Switch on Record routes in Settings to see trips on a map.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        Panel {
            val fuel = shown.fuelUsedL?.takeIf { it > 0 }
            listOf(
                listOf(duration(shown.movingS) to "driving", "${number(shown.avgSpeedKmh, 0)} km/h" to "average speed"),
                listOf(
                    (fuel?.let { number(it, 2) + " l" } ?: "–") to "fuel used",
                    (shown.consumptionLPer100km?.let { number(it, 1) + " l" } ?: "–") to "per 100 km",
                ),
                listOf(
                    "${number(shown.maxRpm.toDouble(), 0)} rpm" to "highest engine speed",
                    (shown.endOdometerKm?.let { number(it, 0) + " km" } ?: "–") to "odometer at the end",
                ),
            ).forEachIndexed { i, row ->
                Row(Modifier.padding(top = if (i > 0) 16.dp else 0.dp)) {
                    row.forEach { (value, label) ->
                        Column(Modifier.weight(1f)) {
                            Text(value, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                            Text(label, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
        }
    }
}

/** Speed along the route, as a plain line. */
@Composable
private fun SpeedChart(route: List<RoutePoint>, modifier: Modifier) {
    val color = MaterialTheme.colorScheme.primary
    val grid = MaterialTheme.colorScheme.outlineVariant
    Canvas(modifier) {
        val t0 = route.first().timeMs.toLong()
        val span = (route.last().timeMs.toLong() - t0).coerceAtLeast(1)
        val top = (route.maxOf { it.speedKmh }).coerceAtLeast(10f)
        drawLine(grid, Offset(0f, size.height), Offset(size.width, size.height), strokeWidth = 1.dp.toPx())
        val path = Path()
        route.forEachIndexed { i, p ->
            val x = (p.timeMs.toLong() - t0).toFloat() / span * size.width
            val y = size.height - p.speedKmh / top * (size.height - 4.dp.toPx())
            if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
        }
        drawPath(path, color, style = Stroke(2.2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
    }
}

/** An OpenStreetMap view (osmdroid): tiles are cached in the app's cache directory. */
@SuppressLint("ClickableViewAccessibility")
private fun mapView(context: Context, interactive: Boolean): MapView {
    Configuration.getInstance().apply {
        userAgentValue = context.packageName // required by the tile servers' usage policy
        osmdroidBasePath = context.cacheDir.resolve("osmdroid")
        osmdroidTileCache = context.cacheDir.resolve("osmdroid/tiles")
    }
    return MapView(context).apply {
        setTileSource(TileSourceFactory.MAPNIK)
        zoomController.setVisibility(CustomZoomButtonsController.Visibility.NEVER)
        setMultiTouchControls(interactive)
        if (interactive) {
            // inside a scrolling page: keep map gestures for the map
            setOnTouchListener { v, _ -> v.parent?.requestDisallowInterceptTouchEvent(true); false }
        } else {
            setOnTouchListener { _, _ -> true }
        }
    }
}

/** A plain round map marker: filled for the end / the car, hollow for the start. */
private fun dot(context: Context, color: Int, filled: Boolean): GradientDrawable {
    val d = context.resources.displayMetrics.density
    return GradientDrawable().apply {
        shape = GradientDrawable.OVAL
        setColor(if (filled) color else Color.WHITE)
        setStroke((3 * d).toInt(), if (filled) Color.WHITE else color)
        setSize((18 * d).toInt(), (18 * d).toInt())
    }
}

/** A trip's route with its start and end. */
@Composable
private fun RouteMap(route: List<RoutePoint>, modifier: Modifier) {
    val color = MaterialTheme.colorScheme.primary.toArgb()
    AndroidView(
        modifier = modifier,
        factory = { mapView(it, interactive = true) },
        update = { map ->
            val points = route.map { GeoPoint(it.lat, it.lon) }
            map.overlays.clear()
            map.overlays += Polyline(map).apply {
                setPoints(points)
                outlinePaint.color = color
                outlinePaint.strokeWidth = 10f
            }
            listOf("Start" to points.first(), "End" to points.last()).forEach { (title, at) ->
                map.overlays += Marker(map).apply {
                    position = at
                    this.title = title
                    icon = dot(map.context, color, filled = title == "End")
                    setAnchor(Marker.ANCHOR_CENTER, Marker.ANCHOR_CENTER)
                }
            }
            map.post { map.zoomToBoundingBox(BoundingBox.fromGeoPoints(points).increaseByScale(1.3f), false) }
            map.invalidate()
        },
        onRelease = { it.onDetach() },
    )
}

/** Where the car was left, on a small map that opens a maps app. */
@Composable
fun ParkedCard(parked: ParkedAt) {
    val context = LocalContext.current
    val color = MaterialTheme.colorScheme.primary.toArgb()
    Panel {
        Row {
            Caption("Parked", Modifier.weight(1f))
            Caption(
                DateUtils.getRelativeTimeSpanString(
                    parked.timeMs.toLong(), System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS,
                ).toString(),
            )
        }
        AndroidView(
            modifier = Modifier.fillMaxWidth().height(150.dp).padding(top = 10.dp).clip(RoundedCornerShape(12.dp)),
            factory = { mapView(it, interactive = false) },
            update = { map ->
                val at = GeoPoint(parked.lat, parked.lon)
                map.overlays.clear()
                map.overlays += Marker(map).apply {
                    position = at
                    title = "Car"
                    icon = dot(map.context, color, filled = true)
                    setAnchor(Marker.ANCHOR_CENTER, Marker.ANCHOR_CENTER)
                }
                map.controller.setZoom(17.0)
                map.controller.setCenter(at)
                map.invalidate()
            },
            onRelease = { it.onDetach() },
        )
        TextButton(
            contentPadding = PaddingValues(0.dp),
            onClick = {
                val uri = Uri.parse("geo:${parked.lat},${parked.lon}?q=${parked.lat},${parked.lon}(Parked car)")
                runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, uri)) }
            },
        ) { Text("Open in a maps app") }
    }
}
