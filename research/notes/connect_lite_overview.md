# What ŠKODA Connect LITE was — context for OpenPlug

A short, factual overview of the app OpenPlug replaces, so the rest of the repo has
context. Not affiliated with or endorsed by ŠKODA, Volkswagen or TEXA; names are used
only to identify the hardware and app this project interoperates with.

## In one paragraph

**ŠKODA Connect LITE** was ŠKODA's free companion app that brought "connected-car"
features to **older vehicles without built-in telematics**. It paired a phone over
Bluetooth with the **"DataPlug"** — a small OBD-II dongle (made by TEXA) plugged into
the car's diagnostic port — read live vehicle data from the car, and combined it with
the phone's GPS to log trips, score driving style, track fuel, remember where you
parked, and remind you about service. ŠKODA **discontinued it on 1 July 2025**; the
dongle hardware still works, which is exactly the gap OpenPlug fills.

## Identity & timeline

- **App:** ŠKODA Connect LITE — Android package `cz.skodaauto.connectlite`, iOS App
  Store id `1454384033` (iOS 13+). Developer of record: ŠKODA AUTO a.s. (the software
  contractor is unknown). The iOS App Store id suggests a launch around early 2019.
- **Hardware:** the TEXA **"DataPlug"** (ŠKODA part no. 000 051 629 A; FCC IDs
  T8RDP15 / T8RDP22), a dual-mode Bluetooth CAN/OBD interface certified for
  Volkswagen, SEAT, Škoda and Audi (TEXA DoC-DATAPLUG-08, software version "1000").
- **Sister apps, same hardware platform:** VW **"We Connect Go"**
  (`de.volkswagen.vwconnect`) and SEAT's **"My SEAT" / "SEAT Plus"** — one TEXA dongle
  platform with brand-specific labels and part numbers (the shared `…051629…` suffix).
- **Not** the same as **"MyŠkoda"**, which is for factory-connected cars (Škoda
  Connect) and does **not** use the DataPlug.
- **Shutdown:** retired 1 July 2025; the Play Store / App Store listings now 404. Only
  the app is gone — the dongle an owner already paid for keeps working.

## What it did — feature list

The app used a freemium model: a core set worked **without an account**, and more
features unlocked after signing in with a **ŠKODA ID**. (Observed firsthand in the
app's navigation drawer: My Škoda, Trips, Statistics, My challenges, Service
appointments, Parking space, Fuel monitor, Vehicle Manual, Assistance Call.)

**Without login**
- **Trips** — an automatic trip logbook (distance, duration, route), stored locally
  on the phone.
- **Driving-style scoring** — a "gentle driving" score per trip.
- **Challenges** — gamified eco-driving tasks that earn points.
- **Assistance Call** — dials roadside assistance / customer service and shares the
  phone's location.
- **Service-partner finder** and **Support**.

**With login (ŠKODA ID)**
- **My Škoda** — vehicle profile (VIN, fuel level, mileage).
- **Fuel Monitor** — a refuelling log with cost and distance, plus automatic refuel
  detection on many models.
- **Parking Space** — your last parking location.
- **Statistics** — aggregates from Trips, Fuel Monitor and Challenges.
- **Service & scheduling** — service-interval tracking and appointment requests to a
  preferred partner.
- **Tyres** — a tyre log and change reminders.
- **Data Backup & Recovery** — cloud backup of the above.
- **Auxiliary heater** (optional) — control of a *dfreeeze* aftermarket heater, via a
  third party's (Digades) server.
- **Lock-screen notifications.**

The home dashboard showed, at a glance: the vehicle model, **fuel range**, **battery
voltage**, and **service intervals** (km / days).

## Where the data came from

- **From the car, via the dongle (OBD/CAN):** VIN, odometer / mileage, fuel level and
  range, battery voltage, engine RPM, temperatures, and service-interval data
  ("vehicle condition"). We confirmed this firsthand by decoding the live traffic —
  see [`live_protocol.md`](live_protocol.md), which lists the actual data objects
  (odometer, range, battery voltage, RPM, service km/days, …).
- **From the phone:** the GPS track, trip start/end addresses, the **parking
  position** (the last GPS fix captured when the Bluetooth link drops), maps, and the
  assistance call.
- **No cloud dependency for reading the car.** The core read path is entirely local,
  phone ↔ dongle; the account and cloud were only for backup, unlocking premium
  features, and partner/aftermarket services (scheduling, the auxiliary heater).

## What it did **not** do

- **No live remote control** of the vehicle — it is read-only diagnostics plus
  logging. (The one exception is the optional third-party auxiliary-heater add-on,
  which is a separate aftermarket device, not a factory function.)
- No evidence it displayed **DTC fault codes** or warning lights; "vehicle condition"
  appears to mean the high-level status values above, not a full diagnostic scan.

## Why this matters for OpenPlug

Everything useful the app did on the car side is **local and read-only over a
standard stack** (Bluetooth → TLS-SRP → protobuf → OBD/CAN). The account/cloud was
never needed to read your own vehicle. So a clean-room reimplementation can restore
the core value — reading live vehicle data (range, fuel, battery, mileage, RPM,
service) and, in time, trip logging — for a dongle the owner already has. The read
path is already decoded ([`live_protocol.md`](live_protocol.md)); what remains is
wiring it into the shared core and the native apps.

---

*Some store-metadata details above come from third-party trackers and search snippets
(the original listings are offline), so treat version/date specifics as approximate.
The feature list is corroborated by ŠKODA's own "Connect LITE app services" page and
by firsthand inspection of the app.*
