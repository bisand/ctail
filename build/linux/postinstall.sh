#!/bin/sh
gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database /usr/share/applications 2>/dev/null || true
update-mime-database /usr/share/mime 2>/dev/null || true

# Register ctail as an available handler for its MIME types so it appears
# in GNOME/KDE "Open With" dialogs without requiring a re-login.
MIMEAPPS="/usr/share/applications/mimeapps.list"
TYPES="text/x-log text/plain text/csv application/x-log"
if [ -f "$MIMEAPPS" ]; then
    for mime in $TYPES; do
        if grep -q "^${mime}=" "$MIMEAPPS" 2>/dev/null; then
            # Append ctail.desktop if not already present
            grep -q "ctail.desktop" "$MIMEAPPS" || \
                sed -i "s|^${mime}=|${mime}=ctail.desktop;|" "$MIMEAPPS"
        else
            echo "${mime}=ctail.desktop;" >> "$MIMEAPPS"
        fi
    done
else
    echo "[Added Associations]" > "$MIMEAPPS"
    for mime in $TYPES; do
        echo "${mime}=ctail.desktop;" >> "$MIMEAPPS"
    done
fi
