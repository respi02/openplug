"""Verify JAR (v1) signature digests of an APK: SF -> MANIFEST.MF -> every entry."""
import base64
import hashlib
import sys
import zipfile


def parse_sections(text):
    sections, cur, last = [], {}, None
    for line in text.replace("\r\n", "\n").split("\n"):
        if line.startswith(" ") and last:
            cur[last] += line[1:]
            continue
        if not line:
            if cur:
                sections.append(cur)
            cur, last = {}, None
            continue
        k, _, v = line.partition(": ")
        cur[k], last = v, k
    if cur:
        sections.append(cur)
    return sections


def digest(alg, data):
    return base64.b64encode(hashlib.new(alg, data).digest()).decode()


ALGS = {"SHA-256": "sha256", "SHA1": "sha1", "SHA-1": "sha1"}

apk, sf_name = sys.argv[1], sys.argv[2]
z = zipfile.ZipFile(apk)
mf_raw = z.read("META-INF/MANIFEST.MF")
sf = parse_sections(z.read(sf_name).decode())
main = sf[0]
ok = True
for key, alg in ALGS.items():
    if f"{key}-Digest-Manifest" in main:
        good = main[f"{key}-Digest-Manifest"] == digest(alg, mf_raw)
        print(f"SF {key}-Digest-Manifest matches MANIFEST.MF: {good}")
        ok &= good
mf = parse_sections(mf_raw.decode())
names = set(n for n in z.namelist() if not n.endswith("/"))
checked = bad = 0
listed = set()
for sec in mf[1:]:
    name = sec["Name"]
    listed.add(name)
    for key, alg in ALGS.items():
        if f"{key}-Digest" in sec:
            checked += 1
            if digest(alg, z.read(name)) != sec[f"{key}-Digest"]:
                bad += 1
                print("MISMATCH", name)
            break
unsigned = sorted(n for n in names - listed if not n.startswith("META-INF/"))
print(f"entries checked: {checked}, mismatches: {bad}, unsigned non-META-INF entries: {len(unsigned)}")
for n in unsigned[:10]:
    print("  unsigned:", n)
print("RESULT:", "OK" if ok and bad == 0 and not unsigned else "FAIL")
