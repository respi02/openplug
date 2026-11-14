#!/usr/bin/env python3
"""Writes altstore.json, the source AltStore and SideStore read to install OpenPlug.

Run after make-ipa.sh, with the tag of the GitHub release the IPA is attached to:

    apple/make-altstore-source.py v0.1.0-alpha.1 "What is new in this version."

The privacy texts are copied from the built app, since AltStore checks that they match.
"""
import datetime
import json
import os
import plistlib
import sys

REPO = "respi02/openplug"
HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.join(HERE, "build/release/Build/Products/Release-iphoneos/OpenPlug.app")
IPA = os.path.join(HERE, "build/OpenPlug.ipa")
OUT = os.path.join(HERE, "..", "altstore.json")

tag, notes = sys.argv[1], sys.argv[2]
with open(os.path.join(APP, "Info.plist"), "rb") as f:
    info = plistlib.load(f)
privacy = {k: v for k, v in sorted(info.items()) if k.startswith("NS") and k.endswith("UsageDescription")}
raw = f"https://raw.githubusercontent.com/{REPO}/main"
icon = f"{raw}/apple/App/Assets.xcassets/AppIcon.appiconset/ios-1024.png"
about = (
    "Trip log and live values for the DataPlug OBD dongle, after the original Connect LITE app "
    "was retired. Talks to the dongle locally over Bluetooth with the PIN from its label: no "
    "account, no server. Open source (MIT). Not affiliated with the vehicle or dongle maker."
)

source = {
    "name": "OpenPlug",
    "identifier": "org.openplug.source",
    "subtitle": "Open-source app for the DataPlug OBD dongle",
    "description": about,
    "iconURL": icon,
    "website": f"https://github.com/{REPO}",
    "tintColor": "#0B6E99",
    "apps": [
        {
            "name": "OpenPlug",
            "bundleIdentifier": info["CFBundleIdentifier"],
            "developerName": "OpenPlug contributors",
            "subtitle": "Trip log for the DataPlug OBD dongle",
            "localizedDescription": about,
            "iconURL": icon,
            "tintColor": "#0B6E99",
            "category": "utilities",
            "screenshots": [],
            "versions": [
                {
                    "version": info["CFBundleShortVersionString"],
                    "buildVersion": info["CFBundleVersion"],
                    "date": datetime.date.today().isoformat(),
                    "localizedDescription": notes,
                    "downloadURL": f"https://github.com/{REPO}/releases/download/{tag}/OpenPlug.ipa",
                    "size": os.path.getsize(IPA),
                    "minOSVersion": info.get("MinimumOSVersion", "17.0"),
                }
            ],
            "appPermissions": {"entitlements": [], "privacy": privacy},
        }
    ],
    "news": [],
}
with open(OUT, "w") as f:
    json.dump(source, f, indent=2, ensure_ascii=False)
    f.write("\n")
print(f"wrote altstore.json for {tag} ({source['apps'][0]['versions'][0]['size']} bytes)")
