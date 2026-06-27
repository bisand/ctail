#!/bin/sh
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
update-mime-database /usr/share/mime 2>/dev/null || true

# Remove ctail.desktop from mimeapps.list associations
MIMEAPPS="/usr/share/applications/mimeapps.list"
if [ -f "$MIMEAPPS" ]; then
    sed -i 's/ctail\.desktop;//g' "$MIMEAPPS"
    # Clean up any lines that are now empty (e.g. "text/x-log=")
    sed -i '/^[^=]*=$/d' "$MIMEAPPS"
fi
