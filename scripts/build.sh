#!/bin/bash
# check if gum is installed
if ! command -v gum &> /dev/null
then
    echo "gum could not be found, please install it"
    exit 1
fi

TEMPLATE="$(gum choose "build (release)" "run (release)" "build (debug)" "run (debug)" "custom")"

# match the template
FLAGS=""
case $TEMPLATE in
    "build (release)")
        FLAGS="build --release"
        ;;
    "run (release)")
        FLAGS="run --release"
        ;;
    "build (debug)")
        FLAGS="build"
        ;;
    "run (debug)")
        FLAGS="run"
        ;;
    "custom")
        FLAGS="$(gum input --placeholder "Put the flags here (e.g. build --release)")"
        ;;
esac

# ask for confirmation
gum confirm "Run cargo $FLAGS?" || exit 1

gum spin --spinner minidot --title "Running cargo $FLAGS (This may take a while!)" -- cargo $FLAGS

gum format -- "# Done!"
