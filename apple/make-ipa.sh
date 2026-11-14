#!/bin/sh
# Builds an unsigned OpenPlug.ipa for sideloading with AltStore or SideStore, which
# sign it with each user's own Apple ID. Run ./build-core.sh and `xcodegen generate`
# first. The result is build/OpenPlug.ipa.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"
xcodebuild -scheme OpenPlug -configuration Release -destination generic/platform=iOS \
    -derivedDataPath build/release CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO build -quiet
stage=$(mktemp -d)
mkdir "$stage/Payload"
cp -R build/release/Build/Products/Release-iphoneos/OpenPlug.app "$stage/Payload/"
rm -f build/OpenPlug.ipa
(cd "$stage" && zip -qry "$here/build/OpenPlug.ipa" Payload)
rm -rf "$stage"
version=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" build/release/Build/Products/Release-iphoneos/OpenPlug.app/Info.plist)
echo "built build/OpenPlug.ipa (version $version, unsigned)"
