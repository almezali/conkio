# Översättningar

Conkio använder Slints gettext-stöd. Källkatalogen är `sv.po` och den
kompilerade katalog som programmet laddar ligger i
`sv/LC_MESSAGES/conkio.mo`.

Efter att `sv.po` har ändrats, kör:

```sh
msgfmt --check --output-file=sv/LC_MESSAGES/conkio.mo sv.po
```

Uppdatera den engelska mallen efter ändringar i `ui/main.slint`:

```sh
slint-tr-extractor ui/main.slint -o translations/conkio.pot
msgmerge --update translations/sv.po translations/conkio.pot
```
