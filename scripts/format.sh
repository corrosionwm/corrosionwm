#!/bin/bash
# check if gum is installed
if ! command -v gum &> /dev/null
then
    echo "gum could not be found, please install it"
    exit 1
fi

COMMANDS="$(gum choose --no-limit "cargo fmt" "cargo fix")"


# ask for confirmation
gum confirm "Run the following command(s)?
$COMMANDS" || exit 1

# run the commands
gum spin --spinner minidot --title "Running the commands (This may take a while!)" -- $COMMANDS

gum format -- "# Done!"

