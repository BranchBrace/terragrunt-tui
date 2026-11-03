# Prépare le run filmé par run.tape : copie la fixture dans un dossier neutre pour que
# l'écran de confirmation et les journaux n'affichent aucun chemin personnel.
$demo = 'C:\Users\Public\ttui-demo'
$exe = (Resolve-Path ./target/release/ttui.exe).Path
Remove-Item -Recurse -Force $demo -ErrorAction Ignore
Copy-Item -Recurse docs/demo/fixture "$demo/infra"
$env:TEMP = $env:TMP = "$demo\tmp"
New-Item -ItemType Directory -Force $env:TEMP | Out-Null
Set-Location "$demo/infra"
Clear-Host
& $exe run --output ../latest.json
