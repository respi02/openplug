# What the "DataPlug by TEXA" is — the hardware OpenPlug talks to

A factual overview of the dongle. Not affiliated with or endorsed by TEXA, ŠKODA or
Volkswagen; names identify the hardware this project interoperates with. Specs come
from public regulatory filings, a published teardown, and our own reverse-engineering
of our own unit — sources that disagree are flagged.

## In one paragraph

The **DataPlug** is a small **Bluetooth-to-CAN/OBD bridge** made by **TEXA S.p.A.**
(Italy) and sold under VW-group brands. It plugs into a car's **OBD-II diagnostic
port**, is powered by it, and exposes the vehicle's data bus to a phone over
Bluetooth. The phone app asks it for diagnostic values (and subscribes to live
signals); the dongle puts those requests on the car's **CAN bus** and streams the
answers back. It is **not** an ELM327-style generic OBD adapter — it speaks a
**proprietary, encrypted protocol**, and it only pairs from inside the brand app using
the **PIN printed on its label**.

## Branding, models, part numbers

- **Maker:** TEXA S.p.A., Monastier di Treviso, Italy (supplier code "TX9" in VW
  literature). Certified for **Volkswagen, SEAT, Škoda and Audi** — one hardware
  platform with brand-specific labels/part numbers (ŠKODA `000 051 629 A`,
  VW `5GV051629…`, SEAT `6F0051629C`).
- **Two generations on file:** the original **DP15** (FCC ID `T8RDP15`, granted 2015)
  and a 2023 respin **DP2.2-488 / "DP22"** (FCC ID `T8RDP22`). A 2017-era ŠKODA
  `000 051 629 A` with software "1000" is most likely DP15-generation hardware.
- EU type: "CAN BUS INTERFACE FOR VEHICLE", software version **1000** (TEXA
  DoC-DATAPLUG-08, 2021).

## Physical & electrical

- **Interface:** OBD-II / ISO 15031-03 plug; powered from the port (**6–16 V**).
  Very low power: standby **< 100 µA**, operating **< 80 mA**. Weight ~**15 g**.
  Operating range −20…70 °C.
- **Status LEDs** (DP22): green = vehicle / configuration state, blue = Bluetooth
  link, red = error, orange (green+red) = firmware update. The LEDs turn off once the
  car starts moving (derived from vehicle speed on the bus — there is no GPS or
  accelerometer inside).
- **Install:** plug it in, pair in-app (~10 min first-time setup). Remove only with
  the ignition off.

## Inside (from a public teardown — not our own)

- **MCU:** STMicroelectronics **STM32F415** (ARM Cortex-M4F @168 MHz, 1 MB flash,
  ~196 KB RAM). Notably this is STM's **crypto variant**, with hardware AES, HASH
  (SHA-1/HMAC) and a true RNG — consistent with the encrypted app↔dongle tunnel.
- **Radio:** Cypress/Infineon **CYW20713**, a **dual-mode Bluetooth** controller
  (Classic BR/EDR 2.1 **and** BLE 4.0), run as an HCI controller with the host
  Bluetooth stack on the STM32. ~0 dBm, PCB antenna. The FCC filings carry two grants
  per device (one DSS/Classic, one DTS/BLE) — the fingerprint of a dual-mode radio.
- **Vehicle bus:** CAN HS (ISO 11898-2), via a CAN transceiver. No external flash,
  GPS or accelerometer — a deliberately minimal design.

The dual-mode radio is why **Android uses Bluetooth Classic (SPP)** and **iOS uses
BLE** to reach the same dongle — no "Made for iPhone" (MFi) needed.

## What it actually does (confirmed by our own reverse-engineering)

The DataPlug is a **gateway**, not a sensor. Over Bluetooth it exposes two things:

1. **Data objects** — named live values (engine RPM, odometer, fuel level / range,
   battery voltage, temperatures, service intervals, VIN, …). The app *subscribes* to
   them at a chosen rate and the dongle streams updates. We decoded this end to end —
   see [`live_protocol.md`](live_protocol.md).
2. **Raw diagnostics (PDU)** — the app can hand it a UDS/OBD request; the dongle puts
   it on the CAN bus and returns the response. This is the generic OBD/UDS path.

A signed, per-model **configuration / mapping table** can be flashed to the dongle to
tell it which bus signals map to which data objects.

## Security (confirmed firsthand)

- **Pairing is in-app only**, using the **PIN on the label** — generic pairing from
  the phone's Bluetooth settings does not work. The PIN is **not** a classic Bluetooth
  passkey; it is an **application-level authentication secret**.
- The app↔dongle link runs a **TLS 1.2 tunnel** with the **TLS-SRP** cipher suite
  `0xC01D` (no certificates): SRP identity `"PA"`, password = the label PIN, via
  **mbedTLS** on the dongle. Inside the tunnel is length-delimited **protobuf**; below
  the tunnel is a small **"DTP" link framing**. Details:
  [`protocol_spec.md`](protocol_spec.md), [`ble_transport.md`](ble_transport.md).
- The dongle's firmware/app stack is by **DAT / CYMOTIVE** (the `com.dat.datlib`
  Java API and the native `dat::` code).

## Why this matters for OpenPlug

Because the DataPlug is a **local, standard-stack gateway** (Bluetooth → TLS-SRP →
protobuf → OBD/CAN) and the only secret is the **PIN the owner already has on the
label**, nothing about reading your own car needs a vendor server or the retired app.
OpenPlug reimplements that local protocol so an owner's dongle stays usable — the read
path is already decoded ([`live_protocol.md`](live_protocol.md)).

---

*Internal-component details (STM32F415, CYW20713) come from a single public teardown
and FCC metadata, so treat them as likely rather than certain for every revision. The
radio modes, the OBD/CAN gateway behaviour, the TLS-SRP tunnel and the PIN-as-secret
are confirmed either by regulatory docs or by our own analysis.*
