# What the original Android app did, and what OpenPlug covers (updated 2026-11-14)

Read from the retired app's own package on our phone: its local database schema, its
screens and its log messages. This is a feature inventory to decide what OpenPlug
should offer — no code is taken from it. Features that depended on the vendor's
servers (accounts, backup, dealer search, service booking) are gone with them and are
out of scope.

## How the app was organised

Everything the user saw was computed **on the phone** from the dongle's value stream
(plus the phone's GPS) and stored in a local database. Screens: home with cards,
trips, refuelling stops, statistics (charts, driving score), dashboard indicators
(warning lights), battery voltage, service intervals, parking position, tyres,
vehicle data, challenges/trophies, service partners and appointments.

## Features and the data behind them

| Feature | What it stored | Needs | OpenPlug |
|---|---|---|---|
| **Live values** | latest reading per data object | dongle | ✅ |
| **Vehicle status** | odometer, fuel, average consumption, battery voltage, days/km since service, next service and oil change — each with a timestamp, so the last values show while disconnected | dongle | ✅ the last value of every data object, with its time |
| **Trips (logbook)** | start/end time, distance, end odometer, duration, start/end fuel and average consumption, start/end position and address, a route map, private/business type and a purpose note, manual trips | dongle + GPS | ◐ time, distance, fuel, speeds, start/end position and address, route map, private/business type and a note, deleting, CSV export; no manual trips |
| **Trip route / samples** | per-sample points along a trip: position, altitude, speed, rpm, acceleration, coolant, oil temperature, boost pressure, intake air temperature, throttle | dongle + GPS | ◐ position and speed |
| **Driving score ("smooth drive")** | per trip: counts of good/bad samples for rpm, acceleration, braking, speed, coolant temperature, and an aggregated result, against a table of thresholds | dongle | ✗ |
| **Refuelling stops** | time, litres added, fuel level after, odometer, position (taken from the last parking position), price and station entered by the user, consumption since the last stop | dongle (+ GPS) | ◐ detection, litres, odometer, position; no price or station |
| **Parking position** | last GPS fix when the link dropped, with address and a map | GPS | ✅ position, map and address |
| **Dashboard indicators** | warning lights by name and status | dongle (group 11) | ✗ not decoded yet |
| **Service intervals** | days/km to next service and oil change, reminders | dongle (groups 8, 9) | ◐ days and km since the last service; with the interval entered by the driver, a countdown and an in-app reminder (group 9 not decoded) |
| **Tyres** | type, mileage, change reminders | user input | ✗ |
| **Challenges / trophies** | goals over trips (distance, rpm, speed, smoothness, places) | trips | ✗ |

## Behaviour worth matching

- **A trip starts** when the vehicle speed first rises above zero. (OpenPlug waits for
  3 km/h held for 3 s, after a standing car was seen to open a trip.)
- **A trip is paused** when the connection to the dongle drops, and **resumes** if the
  car reconnects in time. A paused trip is **finished automatically after 15 minutes**
  (a user setting, default 15), ending at the moment it was paused.
- **A refuel is detected** from the first fuel reading of a new connection: if the
  level rose by more than a threshold since the last reading, a stop is recorded
  (the threshold is 1 when the level is in litres, 10 on the platform that reports
  percent).
- The app **reconnected on its own** whenever the dongle came into reach, and kept
  recording with the screen off.
- Several features were behind a registration wall in the app, but all were computed
  locally.

## What OpenPlug implements from this

- `core/src/journal.rs`: the trip state machine (start on speed, pause on link loss,
  resume, finish after 15 minutes) and refuel detection, with distance from the
  speed integral cross-checked against the odometer, moving time, maximum speed and
  rpm, and fuel used from the tank level; the last value of every data object; from
  position fixes, each trip's route, its start and end, the parking place and the
  place of a refuel; CSV export. Shared by both apps.
- Both apps: a Car tab (live values, or the last known state and the parking place),
  a Trips tab (period summary, trips by day with refuels, trip detail with the route
  on a map), automatic reconnection and running in the background, and the trip in
  progress as a Live Activity (iOS) or live notification (Android).

## Candidates for later, roughly by value

1. Manual trips.
2. Service reminders as system notifications; the car's own service interval (group 9).
3. Warning lights (needs group 11 decoded).
4. Driving score and richer per-trip samples (rpm, temperatures along the route).
5. Refuel price and station; tyres.
