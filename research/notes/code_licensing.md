# OpenPlug — licence kódu a právní použitelnost (2026-11-12)

## 1. Závislosti jádra (`core/`) — vše permisivní, OK
| Crate | Licence |
|---|---|
| num-bigint, num-traits, num-integer | MIT OR Apache-2.0 |
| sha1, digest, block-buffer, crypto-common, cpufeatures, cfg-if | MIT OR Apache-2.0 |
| thiserror, hex | MIT OR Apache-2.0 |
| generic-array | MIT |

Žádný copyleft (GPL/LGPL/MPL). Můžeme je použít v otevřeném i uzavřeném projektu, komerčně, na iOS i Android. Jediná povinnost: zachovat texty licencí a copyright notice (standardní „third-party licenses“ obrazovka v appce). OpenPlug sám navržen jako **MIT OR Apache-2.0** (viz `core/Cargo.toml`).

## 2. Standardy, které implementujeme — volně implementovatelné
- **SRP-6a / TLS-SRP**: RFC 2945 / RFC 5054 (IETF, volné).
- **TLS 1.2**: RFC 5246 (volné). AES, HMAC, SHA — veřejné standardy.
- **protobuf**: formát Google, implementace vlastní (nebo `prost`, Apache-2.0).
- **OBD-II / UDS / KWP / CAN**: ISO 15765, ISO 14229, ISO 14230 — otevřené normy. Samotné ISO dokumenty jsou placené, ale protokoly implementovat smíme.

Nic z toho není TEXA/Cymotive duševní vlastnictví — jsou to otevřené standardy.

## 3. Reimplementace protokolu DataPlugu — zákonná interoperabilita
- **Nepřebíráme cizí kód.** OpenPlug je čistá reimplementace *protokolu* (chování na drátě), ne kopie `libdongles.so`/`libmbedtls.so`. Protokoly, rozhraní a „metody fungování“ nejsou chráněné autorským právem.
- **EU rámec:** Směrnice 2009/24/ES o právní ochraně počítačových programů — čl. 1 odst. 2 vylučuje z ochrany myšlenky, principy a **rozhraní**; čl. 6 výslovně povoluje dekompilaci nezbytnou pro **interoperabilitu**. ČR: § 66 odst. 1 písm. d) a § 66 odst. 4 AZ (viz [[legal]]).
- **Vlastnictví:** uživatel vlastní dongle, analyzuje vlastní provoz vlastního zařízení s vlastními daty z vlastního auta. To je nejsilnější poloha pro interoperabilitu.
- **TLS-SRP jako „ochranné opatření“ (TPM):** teoreticky by někdo mohl TLS-SRP označit za technické ochranné opatření a namítat jeho obcházení. Proti tomu stojí: (a) účelem je čistě interoperabilita s vlastním HW, ne přístup k cizímu chráněnému obsahu; (b) heslo (PIN) je legitimně v držení majitele (na štítku); (c) EU interoperabilní výjimka. Riziko nízké, ale ne nulové — proto držet striktně interoperabilní rámec.

## 4. Pravidla (co NESMÍ do repozitáře ani do distribuce)
- Binárky TEXA/Cymotive (`lib*.so`), APK, firmware, konfigurační blob, podpisy.
- `libkeys.so` tajemství (API klíče Phrase/Adjust, heslo k DB).
- PIN dongle a fotky štítku s PINem.
- Ochranné známky: projekt se jmenuje OpenPlug; v popisu nanejvýš „kompatibilní se Škoda DataPlug“. Žádná loga Škoda/VW/TEXA.

## Závěr
Kód můžeme **legálně použít i zveřejnit** (permisivní závislosti, otevřené standardy, čistá reimplementace pro interoperabilitu s vlastním HW). Podmínky: držet interoperabilní účel, nedistribuovat cizí binárky/klíče/PIN, nepoužívat ochranné známky. Před veřejným releasem doporučeno krátké právní přehlédnutí kvůli TPM nuanci.
