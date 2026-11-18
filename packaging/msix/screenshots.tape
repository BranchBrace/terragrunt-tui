# Captures de la fiche Microsoft Store (1920x1080, au-dessus du minimum 1366x768 du Store).
# Régénérer depuis la racine du dépôt, sous Windows : `cargo build --release --locked --bin terragrunt-tui` puis
# `vhs packaging/msix/screenshots.tape`. Le GIF de sortie ne sert pas : seuls les PNG comptent.
Output target/msix/screenshots.gif

Set Shell "powershell"
Set FontSize 22
Set Width 1920
Set Height 1080
Set Padding 24
Set TypingSpeed 40ms

Hide
Type "cls; ./target/release/terragrunt-tui.exe show --run docs/specs/fixtures/demo-run.json"
Enter
Sleep 10s
Show
Down
Down
Down
Sleep 2s
Screenshot packaging/msix/screenshots/1-modules.png
Tab
Down
Sleep 2s
Screenshot packaging/msix/screenshots/2-ressources.png
Tab
Sleep 2.5s
Screenshot packaging/msix/screenshots/3-modifications.png
Type "d"
Sleep 2.5s
Screenshot packaging/msix/screenshots/4-diff.png
Escape
Sleep 500ms
Escape
Sleep 500ms
Type "/"
Type "app"
Sleep 2s
Screenshot packaging/msix/screenshots/5-recherche.png
Enter
Sleep 2s
Type "q"
Sleep 500ms
