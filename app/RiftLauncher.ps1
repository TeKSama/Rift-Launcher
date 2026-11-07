# Rift Launcher - lanceur de jeux recomp, sans installation de dependances.
# made by TeK_Sama
# Un petit serveur local (Windows PowerShell 5.1) sert l'interface du dossier
# "web" dans une fenetre d'appli Edge, et s'occupe des jeux, saves et mods.

param([switch]$AfterUpdate)   # passe par l'installateur apres une mise a jour

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$AppName    = 'Rift Launcher'
$Port       = 47903
$Root       = Split-Path -Parent $MyInvocation.MyCommand.Path
# Version : une seule source, le fichier version.txt a cote du script.
$AppVersion = '0.0.0'
$VersionFile = Join-Path $Root 'version.txt'
if (Test-Path -LiteralPath $VersionFile) { $AppVersion = ([IO.File]::ReadAllText($VersionFile)).Trim() }
# Mises a jour : la derniere release GitHub du projet contient l'installateur et latest.json.
$UpdateRepo     = 'TeKSama/Rift-Launcher'
$UpdateManifest = "https://github.com/$UpdateRepo/releases/latest/download/latest.json"
# Statut Discord : ID de l'application Discord « Rift Launcher » (discord.com/developers).
# Peut etre remplace dans les reglages (champ avance).
$DiscordClientId = '1568357665183375550'
$WebDir     = Join-Path $Root 'web'
$DataDir    = Join-Path $env:APPDATA 'RiftLauncher'
$LibFile    = Join-Path $DataDir 'library.json'
$TokenFile  = Join-Path $DataDir 'token.txt'
$BackupRoot = Join-Path $DataDir 'sauvegardes'
$BaseUrl    = "http://localhost:$Port/"
$Utf8       = New-Object System.Text.UTF8Encoding($false)
$MaxBackups = 10
$IdleExitSeconds = 150
$DisabledSuffix  = '_desactives'
$LogDir       = Join-Path $DataDir 'logs'
$PresetDir    = Join-Path $Root 'preset'
$CoverDir     = Join-Path $DataDir 'jaquettes'
$SoundDir     = Join-Path $DataDir 'sons'
$SettingsFile = Join-Path $DataDir 'settings.json'
$BuiltinCoverDir = Join-Path $WebDir 'covers'
$ImageExt = @('.jpg', '.jpeg', '.png', '.webp', '.gif', '.svg')
$SoundExt = @('.mp3', '.wav', '.ogg', '.m4a')
$Mime = @{ '.jpg' = 'image/jpeg'; '.jpeg' = 'image/jpeg'; '.png' = 'image/png'; '.webp' = 'image/webp'
           '.gif' = 'image/gif'; '.svg' = 'image/svg+xml'; '.mp3' = 'audio/mpeg'; '.wav' = 'audio/wav'
           '.ogg' = 'audio/ogg'; '.m4a' = 'audio/mp4' }

# SteamGridDB exige TLS 1.2 (pas active par defaut dans Windows PowerShell 5.1).
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

foreach ($d in @($DataDir, $CoverDir, (Join-Path $CoverDir 'jeux'), $SoundDir, (Join-Path $SoundDir 'jeux'), $LogDir)) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
}

# ---------------------------------------------------------------- journal
# Un fichier par jour dans AppData\Roaming\RiftLauncher\logs (les 10 derniers sont gardes).
# Le niveau DEBUG n'est ecrit que si "Journal detaille" est active dans les reglages.
$script:DebugLog = $false
function Write-Log([string]$level, [string]$message) {
    if ($level -eq 'DEBUG' -and -not $script:DebugLog) { return }
    try {
        $now = Get-Date
        $line = '[' + $now.ToString('yyyy-MM-dd HH:mm:ss.fff') + '] [' + $level.PadRight(5) + '] ' + $message
        $file = Join-Path $LogDir ('rift-' + $now.ToString('yyyy-MM-dd') + '.log')
        [IO.File]::AppendAllText($file, $line + "`r`n", $Utf8)
    } catch { }
}

function Log-Error([string]$where, $err) {
    $msg = $where + ' : ' + $err.Exception.Message
    if ($err.InvocationInfo -and $err.InvocationInfo.PositionMessage) {
        $msg += ' | ' + (($err.InvocationInfo.PositionMessage -split "`r?`n")[0]).Trim()
    }
    if ($err.ScriptStackTrace) { $msg += ' | pile : ' + ($err.ScriptStackTrace -replace "`r?`n", ' <- ') }
    Write-Log 'ERROR' $msg
}

function Rotate-Logs {
    Get-ChildItem -LiteralPath $LogDir -Filter 'rift-*.log' -File | Sort-Object Name -Descending |
        Select-Object -Skip 10 | ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }
}

# Toute erreur non prevue est ecrite dans le journal, puis un message explique ou le trouver.
trap {
    Log-Error 'ERREUR FATALE' $_
    try {
        [System.Windows.Forms.MessageBox]::Show(
            "Rift Launcher a rencontré une erreur et doit s'arrêter.`n`nLe journal est dans :`n$LogDir`n`nEnvoie ce dossier (ou un rapport créé depuis les Réglages) pour qu'on regarde le problème.",
            'Rift Launcher', 'OK', 'Error') | Out-Null
    } catch { }
    break
}

# ---------------------------------------------------------------- licences
# Une licence est reconnue grace a des mots-cles cherches dans le nom du jeu et
# dans le chemin de son .exe (nom du dossier). L'ordre compte : le plus precis d'abord.
$Franchises = @(
    @{ key = 'mariokart';    label = 'Mario Kart';      words = @('mario kart', 'mariokart', 'spaghettikart', 'spaghetti kart', 'mk64') },
    @{ key = 'drmario';      label = 'Dr. Mario';       words = @('dr. mario', 'dr mario', 'drmario') },
    @{ key = 'zelda';        label = 'Zelda';           words = @('zelda', 'hyrule', 'ocarina', 'majora', 'harkinian', 'breath of the wild', 'botw', 'tears of the kingdom', 'totk', 'totknatif', 'twilight princess', 'wind waker', 'savage wind', 'savagewind', 'skyward sword') },
    @{ key = 'mario';        label = 'Mario';           words = @('mario', 'sm64', 'peach', 'luigi', 'yoshi') },
    @{ key = 'splatoon';     label = 'Splatoon';        words = @('splatoon', 'splatpc') },
    @{ key = 'metroid';      label = 'Metroid';         words = @('metroid', 'samus') },
    @{ key = 'kirby';        label = 'Kirby';           words = @('kirby') },
    @{ key = 'starfox';      label = 'Star Fox';        words = @('star fox', 'starfox', 'starship', 'sf64') },
    @{ key = 'sonic';        label = 'Sonic';           words = @('sonic') },
    @{ key = 'banjo';        label = 'Banjo-Kazooie';   words = @('banjo', 'kazooie', 'tooie') },
    @{ key = 'perfectdark';  label = 'Perfect Dark';    words = @('perfect dark', 'perfectdark') },
    @{ key = 'goemon';       label = 'Goemon';          words = @('goemon', 'mystical ninja') },
    @{ key = 'donkeykong';   label = 'Donkey Kong';     words = @('donkey kong', 'dk64') },
    @{ key = 'pokemon';      label = 'Pokémon';         words = @('pokemon', 'pokémon') },
    @{ key = 'smash';        label = 'Smash Bros.';     words = @('smash') },
    @{ key = 'fzero';        label = 'F-Zero';          words = @('f zero', 'fzero') },
    @{ key = 'animalcrossing'; label = 'Animal Crossing'; words = @('animal crossing') },
    @{ key = 'fireemblem';   label = 'Fire Emblem';     words = @('fire emblem') },
    @{ key = 'xenoblade';    label = 'Xenoblade';       words = @('xenoblade') }
)

# Les 15 jaquettes integrees (dossier web\covers), reconnues par mots-cles.
$BuiltinCovers = @(
    @{ key = 'mk64';     words = @('mario kart', 'mariokart', 'spaghettikart', 'spaghetti kart', 'mk64') },
    @{ key = 'drmario';  words = @('dr. mario', 'dr mario', 'drmario') },
    @{ key = 'botw';     words = @('breath of the wild', 'botw', 'savage wind', 'savagewind') },
    @{ key = 'totk';     words = @('tears of the kingdom', 'totk', 'totknatif') },
    @{ key = 'oot';      words = @('ocarina', 'ship of harkinian') },
    @{ key = 'mm';       words = @('majora', '2 ship', '2ship', 'zelda64recomp') },
    @{ key = 'tp';       words = @('twilight princess', 'dusk') },
    @{ key = 'ww';       words = @('wind waker') },
    @{ key = 'sm64';     words = @('mario 64', 'super mario 64', 'sm64') },
    @{ key = 'sf64';     words = @('star fox', 'starfox', 'starship', 'sf64') },
    @{ key = 'splatoon'; words = @('splatoon', 'splatpc') },
    @{ key = 'sonicu';   words = @('unleashed') },
    @{ key = 'banjo';    words = @('banjo', 'kazooie') },
    @{ key = 'pd';       words = @('perfect dark', 'perfectdark') },
    @{ key = 'goemon';   words = @('goemon', 'mystical ninja') }
)

# Jeux proposes au premier lancement. Les chemins sont devines a partir du
# dossier de l'utilisateur ; s'ils n'existent pas, la carte affiche "A configurer".
function Guess-Path([string]$relative) {
    $full = Join-Path $env:USERPROFILE $relative
    if (Test-Path -LiteralPath $full) { return $full }
    return ''
}
$Defaults = @(
    @{ name    = 'Zelda: Tears of the Kingdom'
       exePath = Guess-Path 'Desktop\totknatif\installfolder\totk.exe'
       saveDir = Guess-Path 'Desktop\totknatif\installfolder\profile\save'
       modsDir = Guess-Path 'Desktop\totknatif\installfolder\mods'
       repoUrl = 'https://github.com/valentderah/totk-pc-recomp' },
    @{ name    = 'Zelda: Breath of the Wild' }
)
$EditFields = @('name', 'exePath', 'args', 'env', 'saveDir', 'modsDir', 'version', 'repoUrl', 'franchise')

# ---------------------------------------------------------------- utilitaires
function Now-Ms { [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }

function Get-Field($obj, [string]$name) {
    if ($null -eq $obj) { return $null }
    if ($obj -is [System.Collections.IDictionary]) { return $obj[$name] }
    $p = $obj.PSObject.Properties[$name]
    if ($p) { return $p.Value }
    return $null
}

function Open-Window([string]$token) {
    $url = $BaseUrl + '?t=' + $token
    $edgeProfile = Join-Path $DataDir 'edge-app'
    # Profil Edge a part : fenetre independante, et les sons de l'intro
    # peuvent jouer sans clic (autoplay autorise).
    $edgeArgs = @(
        "--app=$url",
        '--window-size=1200,800',
        "--user-data-dir=`"$edgeProfile`"",
        '--no-first-run',
        '--no-default-browser-check',
        '--disable-sync',
        '--disable-features=msImplicitSignin,msEdgeSyncConsentPrompt',
        '--autoplay-policy=no-user-gesture-required'
    )
    try {
        Start-Process -FilePath 'msedge.exe' -ArgumentList $edgeArgs
    } catch {
        Write-Log 'WARN' ('Edge introuvable, ouverture dans le navigateur par defaut : ' + $_.Exception.Message)
        Start-Process $url   # navigateur par defaut si Edge est absent
    }
}

function Safe-Name([string]$name) {
    $n = ($name -replace '[<>:"/\\|?*\x00-\x1f]', '_').Trim()
    if (-not $n) { $n = 'jeu' }
    return $n
}

function Parse-Env([string]$text) {
    $result = @{}
    foreach ($raw in ($text -split "`r?`n")) {
        $line = $raw.Trim()
        if (-not $line -or $line.StartsWith('#')) { continue }
        $eq = $line.IndexOf('=')
        if ($eq -le 0) { continue }
        $result[$line.Substring(0, $eq).Trim()] = $line.Substring($eq + 1).Trim()
    }
    return $result
}

# ---------------------------------------------------------------- jaquettes et sons
function Slug([string]$text) {
    $s = $text.ToLowerInvariant().Normalize([Text.NormalizationForm]::FormD)
    $s = [regex]::Replace($s, '\p{Mn}', '')            # retire les accents
    $s = [regex]::Replace($s, '[^a-z0-9]+', '-').Trim('-')
    return $s
}

function Haystack($g) {
    # Nom du jeu + chemin de l'exe (noms de dossiers), en minuscules.
    $h = (' ' + $g.name + ' ' + $g.exePath + ' ').ToLowerInvariant()
    return ($h -replace '[_\-\\/]+', ' ')
}

function Has-Word([string]$hay, [string[]]$words) {
    foreach ($w in $words) {
        if ($hay -match ('(^|[^a-z0-9])' + [regex]::Escape($w) + '($|[^a-z0-9])')) { return $true }
    }
    return $false
}

function Get-Franchise($g) {
    if ($g.franchise) {
        foreach ($f in $Franchises) { if ($f.key -eq $g.franchise) { return $f } }
        return @{ key = (Slug $g.franchise); label = $g.franchise }
    }
    $hay = Haystack $g
    foreach ($f in $Franchises) { if (Has-Word $hay $f.words) { return $f } }
    return $null
}

function Get-BuiltinKey($g) {
    $hay = Haystack $g
    foreach ($c in $BuiltinCovers) { if (Has-Word $hay $c.words) { return $c.key } }
    return $null
}

# Cherche dans un dossier un fichier dont le nom correspond a l'un des noms donnes.
function Find-Asset([string]$dir, [string[]]$names, [string[]]$exts) {
    if (-not (Test-Path -LiteralPath $dir)) { return $null }
    $files = @(Get-ChildItem -LiteralPath $dir -File | Where-Object { $exts -contains $_.Extension.ToLowerInvariant() })
    foreach ($n in $names) {
        if (-not $n) { continue }
        foreach ($f in $files) { if ((Slug $f.BaseName) -eq $n) { return $f.FullName } }
    }
    return $null
}

function Asset-Names($g) {
    $f = Get-Franchise $g
    $names = @((Slug $g.name), (Get-BuiltinKey $g))
    if ($f) { $names += $f.key }
    return $names
}

# Ordre : image choisie pour ce jeu > dossier "jaquettes" (nom du jeu, puis licence) > jaquette integree.
function Resolve-Cover($g) {
    if ($g.cover -and (Test-Path -LiteralPath $g.cover -PathType Leaf)) { return @{ path = $g.cover; source = 'perso' } }
    $found = Find-Asset $CoverDir (Asset-Names $g) $ImageExt
    if ($found) { return @{ path = $found; source = 'dossier' } }
    $key = Get-BuiltinKey $g
    if ($key) {
        $builtin = Join-Path $BuiltinCoverDir ($key + '.svg')
        if (Test-Path -LiteralPath $builtin) { return @{ path = $builtin; source = 'integree' } }
    }
    return $null
}

# Sons : fichier choisi pour le jeu > sous-dossier (ex. sons\zelda\ : un son au hasard
# parmi tous ceux du dossier) > fichier seul (ex. sons\zelda.mp3).
function Resolve-Sounds($g) {
    if ($g.sound -and (Test-Path -LiteralPath $g.sound -PathType Leaf)) { return @{ paths = @($g.sound); source = 'perso' } }
    $names = Asset-Names $g
    if (Test-Path -LiteralPath $SoundDir) {
        $dirs = @(Get-ChildItem -LiteralPath $SoundDir -Directory | Where-Object { $_.Name -ne 'jeux' })
        foreach ($n in $names) {
            if (-not $n) { continue }
            foreach ($d in $dirs) {
                if ((Slug $d.Name) -ne $n) { continue }
                $files = @(Get-ChildItem -LiteralPath $d.FullName -File |
                    Where-Object { $SoundExt -contains $_.Extension.ToLowerInvariant() } | Sort-Object Name |
                    ForEach-Object { $_.FullName })
                if ($files.Count) { return @{ paths = $files; source = 'aleatoire' } }
            }
        }
    }
    $found = Find-Asset $SoundDir $names $SoundExt
    if ($found) { return @{ paths = @($found); source = 'dossier' } }
    return $null
}

function Version-Tag([string]$path) {
    return [string](Get-Item -LiteralPath $path).LastWriteTimeUtc.Ticks
}

function Load-Settings {
    if (Test-Path -LiteralPath $SettingsFile) {
        try { return [IO.File]::ReadAllText($SettingsFile, $Utf8) | ConvertFrom-Json } catch { }
    }
    return [pscustomobject]@{ sgdbKey = '' }
}

function Save-Settings($s) {
    [IO.File]::WriteAllText($SettingsFile, (ConvertTo-Json -InputObject $s -Depth 4), $Utf8)
}

# Parametres generaux (volumes, sons, intro, journal detaille).
$DefaultPrefs = [ordered]@{ volume = 80; uiVolume = 70; gameVolume = 100; hoverSounds = $true; intro = $true; debugLog = $false; discord = $true; recompCheck = $true; homeToggle = $true; captureButton = $true; lang = 'auto' }

function Get-Prefs {
    $p = [ordered]@{}
    foreach ($k in $DefaultPrefs.Keys) { $p[$k] = $DefaultPrefs[$k] }
    $saved = Get-Field (Load-Settings) 'prefs'
    if ($saved) {
        foreach ($k in $DefaultPrefs.Keys) { $v = Get-Field $saved $k; if ($null -ne $v) { $p[$k] = $v } }
    }
    return $p
}

function Save-Prefs($incoming) {
    $p = Get-Prefs
    foreach ($k in @('volume', 'uiVolume', 'gameVolume')) {
        $v = Get-Field $incoming $k
        if ($null -ne $v) { $p[$k] = [int][math]::Max(0, [math]::Min(100, [double]$v)) }
    }
    foreach ($k in @('hoverSounds', 'intro', 'debugLog', 'discord', 'recompCheck', 'homeToggle', 'captureButton')) {
        $v = Get-Field $incoming $k
        if ($null -ne $v) { $p[$k] = [bool]$v }
    }
    $lang = [string](Get-Field $incoming 'lang')
    if ($lang -in @('auto', 'fr', 'en', 'es', 'zh', 'hi', 'ar')) { $p.lang = $lang }
    $s = Load-Settings
    $s | Add-Member -NotePropertyName prefs -NotePropertyValue $p -Force
    Save-Settings $s
    $script:DebugLog = [bool]$p.debugLog
    Sync-Pad
    return $p
}

function Write-Readmes {
    $keys = ($Franchises | ForEach-Object { '  ' + $_.key + '   (' + $_.label + ')' }) -join "`r`n"
    $coverHelp = @"
JAQUETTES DE RIFT LAUNCHER
==========================

Format conseille : portrait 600 x 900 pixels (rapport 2:3), en JPG, PNG ou WebP.
Une image plus grande (1200 x 1800) marche aussi. Taille maximum : 10 Mo.
Une image d'un autre format sera recadree au centre.

Comment ca marche :
- Nomme l'image comme une licence pour l'appliquer a tous ses jeux.
  Exemple : zelda.png s'affiche sur tous les jeux Zelda.
- Ou nomme-la comme le jeu pour un seul jeu.
  Exemple : zelda-tears-of-the-kingdom.jpg
- Une image choisie dans les parametres d'un jeu passe avant ce dossier.

Noms de licences reconnus :
$keys
"@
    $soundHelp = @"
SONS DE RIFT LAUNCHER
=====================

Le son est joue quand tu selectionnes un jeu et quand tu le lances.

Format conseille : MP3, WAV ou OGG, 1 a 3 secondes, 2 Mo maximum.

Comment ca marche :
- Nomme le son comme une licence pour tous ses jeux. Exemple : zelda.mp3
- Plusieurs sons au hasard : cree un dossier au nom de la licence et mets-y
  tous les sons. Exemple : sons\zelda\hey.mp3, sons\zelda\listen.mp3...
  Un son different est tire au hasard a chaque fois.
- Ou nomme-le comme le jeu pour un seul jeu. Exemple : zelda-tears-of-the-kingdom.mp3
- Un son choisi dans les parametres d'un jeu passe avant ce dossier.
- Sans fichier, Rift Launcher joue son propre petit son pour la licence.

Noms de licences reconnus :
$keys
"@
    $c = Join-Path $CoverDir 'LISEZMOI.txt'
    $s = Join-Path $SoundDir 'LISEZMOI.txt'
    if (-not (Test-Path -LiteralPath $c)) { [IO.File]::WriteAllText($c, $coverHelp, $Utf8) }
    [IO.File]::WriteAllText($s, $soundHelp, $Utf8)   # toujours a jour
}

# ---------------------------------------------------------------- bibliotheque
$Games   = New-Object System.Collections.Generic.List[object]
$Running = @{}   # id -> @{ Proc; LastTick }

function New-Game($src, [bool]$keepStats) {
    $g = [ordered]@{}
    $id = [string](Get-Field $src 'id')
    if (-not $id) { $id = [guid]::NewGuid().ToString() }
    $g.id = $id
    foreach ($f in $EditFields) { $g[$f] = ([string](Get-Field $src $f)).Trim() }
    if (-not $g.name) { $g.name = 'Sans nom' }
    $g.cover = [string](Get-Field $src 'cover')
    $g.sound = [string](Get-Field $src 'sound')
    $g.coverRemote = [string](Get-Field $src 'coverRemote')   # adresse en ligne de la jaquette (statut Discord)
    $g.playSeconds = 0.0
    $g.lastPlayed  = $null
    $g.sessions    = 0
    if ($keepStats) {
        $ps = Get-Field $src 'playSeconds'; if ($ps) { $g.playSeconds = [double]$ps }
        $lp = Get-Field $src 'lastPlayed';  if ($lp) { $g.lastPlayed  = [int64]$lp }
        $se = Get-Field $src 'sessions';    if ($se) { $g.sessions    = [int]$se }
    }
    return $g
}

function Find-Game([string]$id) {
    foreach ($g in $Games) { if ($g.id -eq $id) { return $g } }
    return $null
}

function Save-Library {
    $json = ConvertTo-Json -InputObject @{ games = @($Games.ToArray()) } -Depth 5
    $tmp = $LibFile + '.tmp'
    [IO.File]::WriteAllText($tmp, $json, $Utf8)
    Move-Item -LiteralPath $tmp -Destination $LibFile -Force
}

function Load-Library {
    # Reprend la bibliotheque de l'ancien "Lanceur TeK" s'il y en a une.
    $old = Join-Path $env:APPDATA 'LanceurTeK\library.json'
    if (-not (Test-Path -LiteralPath $LibFile) -and (Test-Path -LiteralPath $old)) {
        Copy-Item -LiteralPath $old -Destination $LibFile
    }
    if (Test-Path -LiteralPath $LibFile) {
        try {
            $raw = [IO.File]::ReadAllText($LibFile, $Utf8) | ConvertFrom-Json
            foreach ($g in @($raw.games)) { if ($g) { $Games.Add((New-Game $g $true)) } }
            return
        } catch { $Games.Clear() }
    }
    # Premier lancement : preglage fourni avec l'installateur (jeux + jaquettes, sans chemins).
    $presetFile = Join-Path $PresetDir 'preset.json'
    if (Test-Path -LiteralPath $presetFile) {
        try {
            $preset = [IO.File]::ReadAllText($presetFile, $Utf8) | ConvertFrom-Json
            foreach ($pg in @($preset.games)) {
                if (-not $pg) { continue }
                $g = New-Game $pg $false
                $g.id = [guid]::NewGuid().ToString()
                # Chemins devines pour les jeux connus (ex. TotK dans Bureau\totknatif).
                foreach ($d in $Defaults) {
                    if ($d.name -ne $g.name) { continue }
                    foreach ($f in @('exePath', 'saveDir', 'modsDir')) { if (-not $g[$f] -and $d[$f]) { $g[$f] = $d[$f] } }
                }
                $coverFile = [string](Get-Field $pg 'coverFile')
                if ($coverFile) {
                    $src = Join-Path (Join-Path $PresetDir 'covers') $coverFile
                    if (Test-Path -LiteralPath $src) {
                        $dest = Join-Path (Join-Path $CoverDir 'jeux') ($g.id + [IO.Path]::GetExtension($src))
                        Copy-Item -LiteralPath $src -Destination $dest -Force
                        $g.cover = $dest
                    }
                }
                $Games.Add($g)
            }
            if (-not (Test-Path -LiteralPath $SettingsFile) -and $preset.prefs) { [void](Save-Prefs $preset.prefs) }
            Write-Log 'INFO' ('Premier lancement : preglage applique (' + $Games.Count + ' jeux).')
        } catch {
            Log-Error 'Preglage' $_
            $Games.Clear()
        }
    }
    if ($Games.Count -eq 0) { foreach ($d in $Defaults) { $Games.Add((New-Game $d $false)) } }
    Save-Library
}

# ---------------------------------------------------------------- temps de jeu compte par le jeu
# Certains portages comptent eux-memes le temps de jeu (TotK recomp 0.3.12+ : profile\playtime.txt,
# « total_seconds N », seulement quand la fenetre du jeu est active). S'il existe, on l'affiche.
$script:PlaytimeCache = @{}   # chemin -> @{ Ticks; Seconds }

function Find-PlaytimeFile($g) {
    $cands = New-Object System.Collections.Generic.List[string]
    try {
        if ($g.exePath) { $cands.Add((Join-Path ([IO.Path]::GetDirectoryName($g.exePath)) 'profile\playtime.txt')) }
        if ($g.saveDir) { $cands.Add((Join-Path ([IO.Path]::GetDirectoryName($g.saveDir.TrimEnd('\'))) 'playtime.txt')) }
    } catch { return $null }
    foreach ($c in $cands) { if (Test-Path -LiteralPath $c -PathType Leaf) { return $c } }
    return $null
}

function Read-GamePlaytime($g) {
    $f = Find-PlaytimeFile $g
    if (-not $f) { return $null }
    try {
        $ticks = (Get-Item -LiteralPath $f).LastWriteTimeUtc.Ticks
        $c = $script:PlaytimeCache[$f]
        if ($c -and $c.Ticks -eq $ticks) { return $c.Seconds }
        $text = [IO.File]::ReadAllText($f)
        if ($text -match '^\s*total_seconds\s+(\d+)') {
            $secs = [double]$Matches[1]
            if (-not $c) { Write-Log 'INFO' ('Temps de jeu de ' + $g.name + ' lu depuis le jeu : ' + $f) }
            $script:PlaytimeCache[$f] = @{ Ticks = $ticks; Seconds = $secs }
            return $secs
        }
    } catch { Write-Log 'DEBUG' ('Lecture du temps de jeu impossible : ' + $f + ' : ' + $_.Exception.Message) }
    return $null
}

function Snapshot {
    $view = New-Object System.Collections.Generic.List[object]
    foreach ($g in $Games) {
        $v = [ordered]@{}
        foreach ($k in $g.Keys) { $v[$k] = $g[$k] }
        $f = Get-Franchise $g
        $v.franchiseKey   = $(if ($f) { $f.key } else { '' })
        $v.franchiseLabel = $(if ($f) { $f.label } else { '' })
        $c = Resolve-Cover $g
        $v.coverUrl    = $(if ($c) { '/img?id=' + $g.id + '&v=' + (Version-Tag $c.path) } else { '' })
        $v.coverSource = $(if ($c) { $c.source } else { '' })
        $s = Resolve-Sounds $g
        $urls = @()
        if ($s) {
            for ($i = 0; $i -lt $s.paths.Count; $i++) {
                $urls += '/snd?id=' + $g.id + '&n=' + $i + '&v=' + (Version-Tag $s.paths[$i])
            }
        }
        $v.soundUrls   = $urls
        $v.soundUrl    = $(if ($urls.Count) { $urls[0] } else { '' })
        $v.soundSource = $(if ($s) { $s.source } else { '' })
        $v.recomp = Recomp-View $g
        $v.launcherSeconds = [double]$g.playSeconds
        $v.playSource = 'launcher'
        $own = Read-GamePlaytime $g
        if ($null -ne $own) { $v.playSeconds = $own; $v.playSource = 'jeu' }
        $view.Add($v)
    }
    return @{ games = @($view.ToArray()); running = @($Running.Keys); version = $AppVersion; update = $script:Update; capture = (Capture-View) }
}

# ---------------------------------------------------------------- jeux en cours
function Add-Time([string]$id) {
    $r = $Running[$id]; $g = Find-Game $id
    if (-not $r -or -not $g) { return }
    $now = [DateTime]::UtcNow
    $g.playSeconds = [double]$g.playSeconds + ($now - $r.LastTick).TotalSeconds
    $r.LastTick = $now
    $g.lastPlayed = Now-Ms
}

function Update-Running {
    $changed = $false
    $ended = $false
    foreach ($id in @($Running.Keys)) {
        $r = $Running[$id]
        if ($r.Proc.HasExited) {
            Add-Time $id
            $code = '?'
            try { $code = [string]$r.Proc.ExitCode } catch { }
            $secs = [int]([DateTime]::UtcNow - $r.Started).TotalSeconds
            $g = Find-Game $id
            if ($r.StopRequested) {
                # Arret via le bouton "Arreter" : Windows force la fermeture (code 1), ce n'est pas un plantage.
                Write-Log 'INFO' ('Fermeture : ' + $g.name + ' | arrete depuis Rift Launcher | ' + $secs + ' s de jeu')
                $Running.Remove($id)
                $changed = $true
                $ended = $true
                continue
            }
            Write-Log 'INFO' ('Fermeture : ' + $g.name + ' | code de sortie ' + $code + ' | ' + $secs + ' s de jeu')
            if ($code -ne '0' -and $code -ne '?') { Write-Log 'WARN' ($g.name + ' s''est ferme avec le code ' + $code + ' (erreur ou plantage du jeu ?)') }
            if ($secs -lt 10) { Write-Log 'WARN' ($g.name + ' s''est ferme ' + $secs + ' s apres le lancement : plantage au demarrage possible.') }
            $Running.Remove($id)
            $changed = $true
            $ended = $true
        } elseif (([DateTime]::UtcNow - $r.LastTick).TotalSeconds -ge 30) {
            Add-Time $id      # enregistre regulierement en cas de plantage
            $changed = $true
        }
    }
    if ($changed) { Save-Library }
    if ($ended) { Sync-Pad; Discord-Update }
}

function Start-Game([string]$id) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    if ($Running.ContainsKey($id)) { return @{ error = 'Ce jeu est déjà lancé.' } }
    if (-not $g.exePath -or -not (Test-Path -LiteralPath $g.exePath -PathType Leaf)) {
        Write-Log 'WARN' ('Lancement impossible, fichier introuvable : ' + $g.name + ' | "' + $g.exePath + '"')
        return @{ error = 'Le fichier du jeu est introuvable. Règle son emplacement dans les paramètres du jeu.' }
    }
    $vars = Parse-Env $g.env
    $old = @{}
    foreach ($k in $vars.Keys) {
        $old[$k] = [Environment]::GetEnvironmentVariable($k, 'Process')
        [Environment]::SetEnvironmentVariable($k, $vars[$k], 'Process')
    }
    try {
        $params = @{
            FilePath         = $g.exePath
            WorkingDirectory = [IO.Path]::GetDirectoryName($g.exePath)
            PassThru         = $true
        }
        if ($g.args) { $params.ArgumentList = $g.args }
        $proc = Start-Process @params
        $null = $proc.Handle   # garde le handle pour lire le code de sortie plus tard
    } catch {
        Log-Error ('Lancement de ' + $g.name) $_
        return @{ error = "Impossible de lancer le jeu : $($_.Exception.Message)" }
    } finally {
        foreach ($k in $old.Keys) { [Environment]::SetEnvironmentVariable($k, $old[$k], 'Process') }
    }
    $Running[$id] = @{ Proc = $proc; LastTick = [DateTime]::UtcNow; Started = [DateTime]::UtcNow }
    $envKeys = (@($vars.Keys) -join ', ')
    Write-Log 'INFO' ('Lancement : ' + $g.name + ' | "' + $g.exePath + '" | arguments : ' + $g.args + ' | variables : ' + $envKeys + ' | PID ' + $proc.Id)
    $g.sessions = [int]$g.sessions + 1
    $g.lastPlayed = Now-Ms
    Save-Library
    Sync-Pad
    Discord-Update
    return @{ ok = $true }
}

function Stop-Game([string]$id) {
    $r = $Running[$id]
    if ($r -and -not $r.Proc.HasExited) {
        # /T ferme aussi les processus enfants (jeu lance via un .bat)
        $r.StopRequested = $true
        & taskkill.exe /PID $r.Proc.Id /T /F 2>$null | Out-Null
        Write-Log 'INFO' ('Arret demande par l''utilisateur : ' + (Find-Game $id).name)
    }
    return @{ ok = $true }
}

# ---------------------------------------------------------------- sauvegardes
function Backup-Game([string]$id) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    if (-not $g.saveDir -or -not (Test-Path -LiteralPath $g.saveDir -PathType Container)) {
        return @{ error = "Choisis d'abord le dossier de sauvegarde du jeu dans ses paramètres." }
    }
    $gameDir = Join-Path $BackupRoot (Safe-Name $g.name)
    New-Item -ItemType Directory -Force -Path $gameDir | Out-Null
    $dest = Join-Path $gameDir (Get-Date).ToString('yyyy-MM-dd_HH-mm-ss')
    Copy-Item -LiteralPath $g.saveDir -Destination $dest -Recurse -Force
    $all = @(Get-ChildItem -LiteralPath $gameDir -Directory | Sort-Object Name)
    if ($all.Count -gt $MaxBackups) {
        $all[0..($all.Count - $MaxBackups - 1)] | ForEach-Object { Remove-Item -LiteralPath $_.FullName -Recurse -Force }
    }
    return @{ ok = $true; path = $dest }
}

# ---------------------------------------------------------------- mods
# Un mod actif est dans le dossier des mods. Pour le couper, on le deplace dans
# le dossier voisin "<nom>_desactives" ; pour le remettre, on le ramene.
function Get-ModDirs($g) {
    if (-not $g.modsDir) { return $null }
    $dir = $g.modsDir.TrimEnd('\', '/')
    $parent = [IO.Path]::GetDirectoryName($dir)
    $leaf = [IO.Path]::GetFileName($dir)
    return @{ on = $dir; off = (Join-Path $parent ($leaf + $DisabledSuffix)) }
}

function List-Mods([string]$id) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    $d = Get-ModDirs $g
    if (-not $d) { return @{ error = 'Choisis d''abord le dossier des mods dans les paramètres du jeu.' } }
    if (-not (Test-Path -LiteralPath $d.on -PathType Container)) {
        return @{ error = "Le dossier des mods n'existe pas : $($d.on)" }
    }
    $mods = New-Object System.Collections.Generic.List[object]
    foreach ($pair in @(@($d.on, $true), @($d.off, $false))) {
        if (-not (Test-Path -LiteralPath $pair[0] -PathType Container)) { continue }
        foreach ($item in Get-ChildItem -LiteralPath $pair[0] -Force) {
            if ($item.Name.StartsWith('.')) { continue }
            $mods.Add([ordered]@{
                name    = $item.Name
                enabled = $pair[1]
                isDir   = $item.PSIsContainer
                date    = [DateTimeOffset]::new($item.LastWriteTimeUtc).ToUnixTimeMilliseconds()
            })
        }
    }
    $sorted = @($mods.ToArray() | Sort-Object { $_.name })
    return @{ dir = $d.on; mods = $sorted }
}

function Set-Mod([string]$id, [string]$name, [bool]$enable) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    $d = Get-ModDirs $g
    if (-not $d) { return @{ error = 'Aucun dossier des mods.' } }
    if (-not $name -or $name -match '[\\/:*?"<>|]' -or $name -eq '.' -or $name -eq '..') {
        return @{ error = 'Nom de mod invalide.' }
    }
    if ($enable) { $from = $d.off; $to = $d.on } else { $from = $d.on; $to = $d.off }
    $src = Join-Path $from $name
    $dst = Join-Path $to $name
    if (-not (Test-Path -LiteralPath $src)) { return @{ error = 'Ce mod a été déplacé ou supprimé. Actualise la liste.' } }
    if (Test-Path -LiteralPath $dst) { return @{ error = "Un mod du même nom existe déjà de l'autre côté : $name" } }
    if ($Running.ContainsKey($id)) { return @{ error = 'Ferme le jeu avant de changer ses mods.' } }
    New-Item -ItemType Directory -Force -Path $to | Out-Null
    try {
        Move-Item -LiteralPath $src -Destination $dst
    } catch {
        Log-Error ('Mod ' + $name) $_
        return @{ error = "Impossible de déplacer le mod : $($_.Exception.Message)" }
    }
    Write-Log 'INFO' ('Mod ' + $(if ($enable) { 'active' } else { 'desactive' }) + ' : ' + $name + ' (' + $g.name + ')')
    return @{ ok = $true }
}

# ---------------------------------------------------------------- jaquettes : actions
function Remove-GameAsset([string]$dir, [string]$id) {
    Get-ChildItem -LiteralPath $dir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.BaseName -eq $id } | ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force }
}

function Pick-File([string]$title, [string]$filter) {
    $owner = New-DialogOwner
    try {
        $d = New-Object System.Windows.Forms.OpenFileDialog
        $d.Title = $title
        $d.Filter = $filter
        if ($d.ShowDialog($owner) -eq 'OK') { return $d.FileName }
    } finally { $owner.Dispose() }
    return $null
}

function Image-Warning([string]$path) {
    try {
        Add-Type -AssemblyName System.Drawing
        $img = [System.Drawing.Image]::FromFile($path)
        $w = $img.Width; $h = $img.Height
        $img.Dispose()
        $ratio = $w / [double]$h
        if ([math]::Abs($ratio - (2.0 / 3.0)) -gt 0.06) {
            return "Ton image fait $w x $h : elle n'est pas au format portrait 2:3, elle sera recadrée au centre."
        }
        if ($w -lt 400) { return "Ton image fait $w x $h : elle risque d'être floue (conseillé : 600 x 900)." }
    } catch { }
    return ''
}

function Set-GameCoverFromFile([string]$id) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    $file = Pick-File 'Choisir une jaquette (portrait 600 x 900 conseillé)' 'Images (*.jpg;*.jpeg;*.png;*.webp;*.gif)|*.jpg;*.jpeg;*.png;*.webp;*.gif'
    if (-not $file) { return @{ ok = $false } }
    if ((Get-Item -LiteralPath $file).Length -gt 10MB) { return @{ error = "L'image dépasse 10 Mo." } }
    $dir = Join-Path $CoverDir 'jeux'
    Remove-GameAsset $dir $id
    $dest = Join-Path $dir ($id + [IO.Path]::GetExtension($file).ToLowerInvariant())
    Copy-Item -LiteralPath $file -Destination $dest -Force
    $g.cover = $dest
    $g.coverRemote = ''
    Save-Library
    return @{ ok = $true; warning = (Image-Warning $dest) }
}

function Set-GameSoundFromFile([string]$id) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    $file = Pick-File 'Choisir un son (1 à 3 secondes conseillé)' 'Sons (*.mp3;*.wav;*.ogg;*.m4a)|*.mp3;*.wav;*.ogg;*.m4a'
    if (-not $file) { return @{ ok = $false } }
    if ((Get-Item -LiteralPath $file).Length -gt 2MB) { return @{ error = 'Le son dépasse 2 Mo : prends un extrait plus court.' } }
    $dir = Join-Path $SoundDir 'jeux'
    Remove-GameAsset $dir $id
    $dest = Join-Path $dir ($id + [IO.Path]::GetExtension($file).ToLowerInvariant())
    Copy-Item -LiteralPath $file -Destination $dest -Force
    $g.sound = $dest
    Save-Library
    return @{ ok = $true }
}

function Clear-GameAsset([string]$id, [string]$kind) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    if ($kind -eq 'sound') { Remove-GameAsset (Join-Path $SoundDir 'jeux') $id; $g.sound = '' }
    else { Remove-GameAsset (Join-Path $CoverDir 'jeux') $id; $g.cover = ''; $g.coverRemote = '' }
    Save-Library
    return @{ ok = $true }
}

# SteamGridDB : base communautaire de jaquettes (cle API gratuite sur steamgriddb.com).
function Sgdb([string]$path) {
    $key = [string](Load-Settings).sgdbKey
    if (-not $key) { throw 'NOKEY' }
    try {
        $r = Invoke-RestMethod -Uri ('https://www.steamgriddb.com/api/v2' + $path) -Headers @{ Authorization = "Bearer $key" } -TimeoutSec 20 -UseBasicParsing
    } catch {
        $code = $null
        if ($_.Exception.Response) { $code = [int]$_.Exception.Response.StatusCode }
        if ($code -eq 401 -or $code -eq 403) { throw 'BADKEY' }
        throw ('Recherche impossible : ' + $_.Exception.Message)
    }
    if ($r.PSObject.Properties['data']) { return @($r.data) }
    return @($r)
}

function Search-Covers([string]$term, [string]$sgdbId) {
    try {
        $games = @()
        if ($term) {
            $games = @(Sgdb ('/search/autocomplete/' + [uri]::EscapeDataString($term)) | Select-Object -First 8 |
                ForEach-Object { [ordered]@{ id = [string]$_.id; name = [string]$_.name } })
        }
        if (-not $sgdbId -and $games.Count) { $sgdbId = $games[0].id }
        $grids = @()
        if ($sgdbId) {
            $grids = @(Sgdb ('/grids/game/' + $sgdbId + '?dimensions=600x900,1200x1800&types=static') | Select-Object -First 36 |
                ForEach-Object { [ordered]@{ thumb = [string]$_.thumb; url = [string]$_.url } })
        }
        return @{ games = $games; gameId = [string]$sgdbId; grids = $grids }
    } catch {
        $m = $_.Exception.Message
        if ($m -eq 'NOKEY') { return @{ error = 'NOKEY' } }
        if ($m -eq 'BADKEY') { Write-Log 'WARN' 'SteamGridDB : cle refusee'; return @{ error = 'Ta clé SteamGridDB est refusée. Vérifie-la dans les réglages.' } }
        Write-Log 'WARN' ('SteamGridDB : ' + $m)
        return @{ error = $m }
    }
}

function Download-Cover([string]$id, [string]$url) {
    $g = Find-Game $id
    if (-not $g) { return @{ error = 'Jeu introuvable.' } }
    if ($url -notmatch '^https://[a-z0-9.-]*steamgriddb\.com/') { return @{ error = 'Adresse d''image refusée.' } }
    $ext = [IO.Path]::GetExtension(([uri]$url).AbsolutePath).ToLowerInvariant()
    if ($ImageExt -notcontains $ext) { $ext = '.png' }
    $dir = Join-Path $CoverDir 'jeux'
    $tmp = Join-Path $dir ($id + '.download')
    try {
        Invoke-WebRequest -Uri $url -OutFile $tmp -UseBasicParsing -TimeoutSec 60
    } catch {
        Log-Error ('Telechargement de jaquette ' + $url) $_
        return @{ error = 'Téléchargement impossible : ' + $_.Exception.Message }
    }
    Write-Log 'INFO' ('Jaquette en ligne choisie pour ' + $g.name + ' : ' + $url)
    if ((Get-Item -LiteralPath $tmp).Length -gt 15MB) { Remove-Item -LiteralPath $tmp -Force; return @{ error = 'Image trop lourde.' } }
    Remove-GameAsset $dir $id
    $dest = Join-Path $dir ($id + $ext)
    Move-Item -LiteralPath $tmp -Destination $dest -Force
    $g.cover = $dest
    $g.coverRemote = $url
    Save-Library
    return @{ ok = $true }
}

# ---------------------------------------------------------------- fenetres Windows
function Show-Picker([string]$kind) {
    $owner = New-DialogOwner
    try {
        if ($kind -eq 'exe') {
            $d = New-Object System.Windows.Forms.OpenFileDialog
            $d.Title = 'Choisir le jeu'
            $d.Filter = 'Jeux (*.exe;*.bat;*.cmd)|*.exe;*.bat;*.cmd|Tous les fichiers (*.*)|*.*'
            if ($d.ShowDialog($owner) -eq 'OK') { return @{ path = $d.FileName } }
        } else {
            $d = New-Object System.Windows.Forms.FolderBrowserDialog
            if ($kind -eq 'mods') { $d.Description = 'Choisir le dossier des mods du jeu' }
            else { $d.Description = 'Choisir le dossier de sauvegarde du jeu' }
            if ($d.ShowDialog($owner) -eq 'OK') { return @{ path = $d.SelectedPath } }
        }
    } finally { $owner.Dispose() }
    return @{ path = $null }
}

# Ouvre un dossier (ou selectionne un fichier) dans l'Explorateur, au premier plan.
function Open-Explorer([string]$path, [switch]$Select) {
    $before = $null
    if ($script:PadReady) { try { $before = [RiftPad]::ExplorerWindows() } catch { } }
    if ($Select) {
        Start-Process explorer.exe -ArgumentList "/select,`"$path`""
        $title = [IO.Path]::GetFileName([IO.Path]::GetDirectoryName($path))
    } else {
        Start-Process explorer.exe -ArgumentList "`"$path`""
        $title = [IO.Path]::GetFileName($path.TrimEnd('\'))
    }
    if ($script:PadReady) { try { [RiftPad]::FocusExplorerSoon($before, $title) } catch { } }
}

# Fenetre support des boites « Parcourir » : visible (minuscule) et mise devant,
# sinon la boite s'ouvre derriere la fenetre du launcher.
function New-DialogOwner {
    $owner = New-Object System.Windows.Forms.Form
    $owner.TopMost = $true
    $owner.ShowInTaskbar = $false
    $owner.FormBorderStyle = 'None'
    $owner.StartPosition = 'Manual'
    $owner.Size = New-Object System.Drawing.Size(1, 1)
    $pos = [System.Windows.Forms.Cursor]::Position
    $owner.Location = New-Object System.Drawing.Point($pos.X, $pos.Y)
    $owner.Opacity = 0
    $owner.Show()
    if ($script:PadReady) { try { [RiftPad]::ForceForeground($owner.Handle) } catch { } }
    $owner.Activate()
    return $owner
}

# ---------------------------------------------------------------- mises a jour
$script:Update = $null          # derniere version trouvee (si plus recente)
$script:UpdateTask = $null      # verification en cours (asynchrone)
$script:QuitForUpdate = $false

function Parse-Version([string]$v) {
    # « v0.3.1 », « 1.2 », « build-45 », « 0.4.0-beta » : on garde le premier groupe de chiffres.
    $m = [regex]::Match([string]$v, '\d+(\.\d+){0,3}')
    if (-not $m.Success) { return [version]'0.0' }
    $txt = $m.Value
    if ($txt -notmatch '\.') { $txt += '.0' }
    $parsed = $null
    if ([version]::TryParse($txt, [ref]$parsed)) { return $parsed }
    return [version]'0.0'
}

function Start-UpdateCheck {
    try {
        $wc = New-Object System.Net.WebClient
        $wc.Headers['User-Agent'] = 'RiftLauncher/' + $AppVersion
        $wc.Headers['Cache-Control'] = 'no-cache'
        $wc.Encoding = [Text.Encoding]::UTF8
        $script:UpdateTask = $wc.DownloadStringTaskAsync([uri]($UpdateManifest + '?t=' + [DateTime]::UtcNow.Ticks))
    } catch { Write-Log 'WARN' ('Verification des mises a jour impossible : ' + $_.Exception.Message) }
}

function Read-UpdateManifest([string]$json) {
    $m = $json | ConvertFrom-Json
    $remote = [string]$m.version
    if ((Parse-Version $remote) -gt (Parse-Version $AppVersion)) {
        $script:Update = [ordered]@{ version = $remote; notes = [string]$m.notes; date = [string]$m.date; url = [string]$m.url; sha256 = [string]$m.sha256 }
        Write-Log 'INFO' ('Mise a jour disponible : ' + $AppVersion + ' -> ' + $remote)
    } else {
        $script:Update = $null
        Write-Log 'DEBUG' ('A jour (' + $AppVersion + ', derniere publiee : ' + $remote + ')')
    }
}

function Poll-UpdateCheck {
    $task = $script:UpdateTask
    if (-not $task -or -not $task.IsCompleted) { return }
    $script:UpdateTask = $null
    if ($task.IsFaulted -or $task.IsCanceled) {
        $err = if ($task.Exception) { $task.Exception.GetBaseException().Message } else { 'annulee' }
        Write-Log 'WARN' ('Verification des mises a jour impossible : ' + $err)
        return
    }
    try { Read-UpdateManifest $task.Result } catch { Log-Error 'Lecture de latest.json' $_ }
}

# Verification immediate (bouton « Verifier » des reglages).
function Check-UpdateNow {
    try {
        $wc = New-Object System.Net.WebClient
        $wc.Headers['User-Agent'] = 'RiftLauncher/' + $AppVersion
        $wc.Encoding = [Text.Encoding]::UTF8
        Read-UpdateManifest ($wc.DownloadString([uri]($UpdateManifest + '?t=' + [DateTime]::UtcNow.Ticks)))
        return @{ ok = $true; current = $AppVersion; update = $script:Update }
    } catch {
        Log-Error 'Verification des mises a jour' $_
        return @{ error = 'Impossible de vérifier les mises à jour (pas de connexion ?).' }
    }
}

function Install-Update {
    $u = $script:Update
    if (-not $u) { return @{ error = 'Aucune mise à jour à installer.' } }
    if ($Running.Count) { return @{ error = 'Ferme ton jeu avant de mettre à jour Rift Launcher.' } }
    $prefix = '^https://github\.com/' + [regex]::Escape($UpdateRepo) + '/releases/download/'
    if ($u.url -notmatch $prefix) { return @{ error = 'Adresse de mise à jour refusée.' } }
    $file = Join-Path $env:TEMP ('RiftLauncherSetup-' + ($u.version -replace '[^0-9.]', '') + '.exe')
    try {
        Write-Log 'INFO' ('Telechargement de la mise a jour ' + $u.version + ' : ' + $u.url)
        $wc = New-Object System.Net.WebClient
        $wc.Headers['User-Agent'] = 'RiftLauncher/' + $AppVersion
        $wc.DownloadFile([uri]$u.url, $file)
        if ($u.sha256) {
            $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
            if ($hash -ne $u.sha256.ToUpperInvariant()) {
                Remove-Item -LiteralPath $file -Force
                Write-Log 'ERROR' ('Mise a jour corrompue : SHA-256 ' + $hash + ' au lieu de ' + $u.sha256)
                return @{ error = 'Le fichier téléchargé est abîmé. Réessaie plus tard.' }
            }
        }
        Start-Process -FilePath $file -ArgumentList '/update'
        Write-Log 'INFO' ('Installateur ' + $u.version + ' lance, arret de Rift Launcher pour la mise a jour.')
        $script:QuitForUpdate = $true
        return @{ ok = $true; version = $u.version }
    } catch {
        Log-Error 'Installation de la mise a jour' $_
        return @{ error = 'Mise à jour impossible : ' + $_.Exception.Message }
    }
}

# ---------------------------------------------------------------- mises a jour des portages (recomps)
# Pour chaque jeu dont la « page du projet » est un depot GitHub, on lit sa derniere
# release (ou son dernier tag) et on la compare a la version indiquee pour le jeu.
$script:RecompInfo  = @{}   # id -> derniere version trouvee
$script:RecompTasks = @{}   # id -> verification en cours (asynchrone)
$script:RecompNextCheck = [DateTime]::MaxValue
$RecompEverySeconds = 6 * 3600

function Repo-Of([string]$url) {
    if ($url -match '^https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(\.git)?(/.*)?$') { return $Matches[1] + '/' + $Matches[2] }
    return $null
}

function New-GitHubClient {
    $wc = New-Object System.Net.WebClient
    $wc.Headers['User-Agent'] = 'RiftLauncher/' + $AppVersion
    $wc.Headers['Accept'] = 'application/vnd.github+json'
    $wc.Encoding = [Text.Encoding]::UTF8
    return $wc
}

function Start-RecompCheck([string]$id, [string]$repo, [string]$kind) {
    $path = if ($kind -eq 'tags') { "/repos/$repo/tags?per_page=1" } else { "/repos/$repo/releases/latest" }
    try {
        $task = (New-GitHubClient).DownloadStringTaskAsync([uri]('https://api.github.com' + $path))
        $script:RecompTasks[$id] = @{ Task = $task; Repo = $repo; Kind = $kind }
    } catch { Write-Log 'WARN' ('Verification du portage ' + $repo + ' impossible : ' + $_.Exception.Message) }
}

function Start-RecompChecks([bool]$force) {
    $script:RecompNextCheck = [DateTime]::UtcNow.AddSeconds($RecompEverySeconds)
    if (-not (Get-Prefs).recompCheck) { return }
    foreach ($g in $Games) {
        $repo = Repo-Of $g.repoUrl
        if (-not $repo) { $script:RecompInfo.Remove($g.id); continue }
        $known = $script:RecompInfo[$g.id]
        if (-not $force -and $known -and $known.repo -eq $repo) { continue }
        if ($script:RecompTasks.ContainsKey($g.id)) { continue }
        Start-RecompCheck $g.id $repo 'release'
    }
}

function Poll-RecompChecks {
    foreach ($id in @($script:RecompTasks.Keys)) {
        $job = $script:RecompTasks[$id]
        if (-not $job.Task.IsCompleted) { continue }
        $script:RecompTasks.Remove($id)
        $g = Find-Game $id
        if (-not $g) { continue }
        if ($job.Task.IsFaulted -or $job.Task.IsCanceled) {
            $code = 0
            $err = 'annulee'
            if ($job.Task.Exception) {
                $base = $job.Task.Exception.GetBaseException()
                $err = $base.Message
                if ($base -is [System.Net.WebException] -and $base.Response) { $code = [int]$base.Response.StatusCode }
            }
            # Pas de release publiee : on regarde les tags.
            if ($code -eq 404 -and $job.Kind -eq 'release') { Start-RecompCheck $id $job.Repo 'tags'; continue }
            if ($code -eq 404) { Write-Log 'DEBUG' ('Portage ' + $job.Repo + ' : aucune version publiee'); continue }
            Write-Log 'WARN' ('Verification du portage ' + $job.Repo + ' impossible : ' + $err)
            continue
        }
        try {
            $data = $job.Task.Result | ConvertFrom-Json
            if ($job.Kind -eq 'tags') {
                $first = @($data)[0]
                if (-not $first) { continue }
                $info = [ordered]@{ repo = $job.Repo; latest = [string]$first.name; url = "https://github.com/$($job.Repo)/tags"; date = ''; title = '' }
            } else {
                $info = [ordered]@{ repo = $job.Repo; latest = [string]$data.tag_name; url = [string]$data.html_url
                                    date = [string]$data.published_at; title = [string]$data.name }
            }
            if (-not $info.latest) { continue }
            if ($info.url -notmatch '^https://github\.com/') { $info.url = "https://github.com/$($job.Repo)/releases" }
            $script:RecompInfo[$id] = $info
            $view = Recomp-View $g
            if ($view.isNew) { Write-Log 'INFO' ('Nouvelle version du portage de ' + $g.name + ' : ' + $info.latest + ' (installee : ' + $g.version + ')') }
            else { Write-Log 'DEBUG' ('Portage de ' + $g.name + ' : derniere version ' + $info.latest) }
        } catch { Log-Error ('Lecture de la version du portage ' + $job.Repo) $_ }
    }
}

function Recomp-View($g) {
    $info = $script:RecompInfo[$g.id]
    if (-not $info -or $info.repo -ne (Repo-Of $g.repoUrl)) { return $null }
    $isNew = $false
    if ($g.version) { $isNew = (Parse-Version $info.latest) -gt (Parse-Version $g.version) }
    return [ordered]@{ latest = $info.latest; url = $info.url; date = $info.date; title = $info.title; isNew = $isNew }
}

# Verification immediate (bouton des reglages) : attend les reponses 10 s maximum.
function Check-RecompsNow {
    $script:RecompInfo = @{}
    Start-RecompChecks $true
    $until = [DateTime]::UtcNow.AddSeconds(10)
    while ($script:RecompTasks.Count -and [DateTime]::UtcNow -lt $until) {
        Start-Sleep -Milliseconds 150
        Poll-RecompChecks
    }
    $found = @($Games | Where-Object { $v = Recomp-View $_; $v -and $v.isNew } | ForEach-Object { $_.name })
    $checked = @($Games | Where-Object { Repo-Of $_.repoUrl }).Count
    return @{ ok = $true; checked = $checked; updates = $found }
}

# « Je l'ai installee » : la version du jeu devient la derniere version publiee.
function Set-RecompDone([string]$id) {
    $g = Find-Game $id
    $v = $(if ($g) { Recomp-View $g } else { $null })
    if (-not $v) { return @{ error = 'Aucune version trouvée pour ce jeu.' } }
    $g.version = ($v.latest -replace '^[vV](?=\d)', '')
    Save-Library
    Write-Log 'INFO' ('Portage de ' + $g.name + ' marque a jour : ' + $g.version)
    return @{ ok = $true }
}

# ---------------------------------------------------------------- statut Discord
# Discord Rich Presence par le tuyau local de Discord (discord-ipc-0..9), sans bibliotheque.
# Trame : code (int32) + longueur (int32) + JSON. 0 = poignee de main, 1 = commande.
$script:Discord = $null          # @{ Pipe; Read; Buf }
$script:DiscordRetry = [DateTime]::MinValue
$script:DiscordShown = ''
$script:DiscordMissingLogged = $false

function Discord-Id {
    $own = [string](Get-Field (Load-Settings) 'discordId')
    if ($own -match '^\d{15,22}$') { return $own }
    return $DiscordClientId
}

function Discord-Send([int]$op, $obj) {
    $pipe = $script:Discord.Pipe
    $data = $Utf8.GetBytes((ConvertTo-Json -InputObject $obj -Depth 8 -Compress))
    $pipe.Write([BitConverter]::GetBytes($op), 0, 4)
    $pipe.Write([BitConverter]::GetBytes([int]$data.Length), 0, 4)
    $pipe.Write($data, 0, $data.Length)
    $pipe.Flush()
}

# Lit ce que Discord renvoie (READY, erreurs) sans jamais bloquer.
function Discord-Drain {
    $d = $script:Discord
    if (-not $d) { return }
    while ($d.Read -and $d.Read.IsCompleted) {
        if ($d.Read.IsFaulted) { Discord-Close 'lecture impossible'; return }
        $n = $d.Read.Result
        if ($n -le 0) { Discord-Close 'Discord a ferme la connexion'; return }
        if ($n -gt 8) {
            $op = [BitConverter]::ToInt32($d.Buf, 0)
            $text = $Utf8.GetString($d.Buf, 8, $n - 8)
            if ($op -eq 2) { Write-Log 'WARN' ('Discord refuse le statut : ' + $text); Discord-Close 'refus'; return }
            if ($text -match '"evt":"ERROR"') { Write-Log 'WARN' ('Discord : ' + $text) }
            elseif ($text -match '"evt":"READY"') { Write-Log 'INFO' 'Discord : connexion acceptee' }
        }
        $d.Read = $d.Pipe.ReadAsync($d.Buf, 0, $d.Buf.Length)
    }
}

function Discord-Close([string]$why) {
    if (-not $script:Discord) { return }
    try { $script:Discord.Pipe.Dispose() } catch { }
    $script:Discord = $null
    $script:DiscordShown = ''
    if ($why) { Write-Log 'DEBUG' ('Discord deconnecte : ' + $why) }
}

function Discord-Connect {
    if ($script:Discord -and $script:Discord.Pipe.IsConnected) { return $true }
    Discord-Close ''
    $client = Discord-Id
    if (-not $client) { return $false }
    for ($i = 0; $i -lt 10; $i++) {
        $name = 'discord-ipc-' + $i
        $exists = $false
        try { $exists = Test-Path -LiteralPath ('\\.\pipe\' + $name) } catch { }
        if (-not $exists) { continue }
        $pipe = New-Object System.IO.Pipes.NamedPipeClientStream('.', $name, [System.IO.Pipes.PipeDirection]::InOut, [System.IO.Pipes.PipeOptions]::Asynchronous)
        try {
            $pipe.Connect(800)
            $buf = New-Object byte[] 65536
            $script:Discord = @{ Pipe = $pipe; Buf = $buf; Read = $null }
            Discord-Send 0 ([ordered]@{ v = 1; client_id = $client })
            $script:Discord.Read = $pipe.ReadAsync($buf, 0, $buf.Length)
            # Attend la reponse READY de Discord avant d'envoyer le statut.
            [void]$script:Discord.Read.Wait(2000)
            Discord-Drain
            if (-not $script:Discord) { return $false }
            Write-Log 'INFO' ('Discord trouve (' + $name + ')')
            $script:DiscordMissingLogged = $false
            return $true
        } catch {
            try { $pipe.Dispose() } catch { }
            $script:Discord = $null
            Write-Log 'DEBUG' ('Discord ' + $name + ' : ' + $_.Exception.Message)
        }
    }
    if (-not $script:DiscordMissingLogged) { Write-Log 'INFO' 'Discord introuvable : statut non affiche (Discord est-il ouvert ?). Nouvel essai toutes les 60 s.' }
    $script:DiscordMissingLogged = $true
    return $false
}

function Discord-Clip([string]$t, [int]$max) {
    if ($t.Length -lt 2) { $t = $t + '  ' }
    if ($t.Length -gt $max) { $t = $t.Substring(0, $max - 1) + '…' }
    return $t
}

# Ce qu'il faut afficher : le dernier jeu lance, sinon « dans les menus » tant que la
# fenetre est ouverte, sinon rien.
function Discord-Wanted {
    if (-not (Get-Prefs).discord -or -not (Discord-Id)) { return '' }
    if ($Running.Count) { return [string](@($Running.Keys | Sort-Object { $Running[$_].Started } -Descending)[0]) }
    if (([DateTime]::UtcNow - $script:LastSeen).TotalSeconds -lt 75) { return 'menu' }   # fenetre reduite : Edge ralentit ses appels
    return ''
}

function Discord-Update {
    try {
        $want = Discord-Wanted
        if (-not $want) { Discord-Close $(if ($script:Discord) { 'rien a afficher' } else { '' }); return }
        $script:DiscordRetry = [DateTime]::UtcNow.AddSeconds(60)
        if (-not (Discord-Connect)) { return }
        if ($want -eq 'menu') {
            $activity = [ordered]@{
                details    = 'Navigue dans les menus'
                timestamps = @{ start = [DateTimeOffset]::new($script:StartedAt).ToUnixTimeMilliseconds() }
                assets     = [ordered]@{ large_image = 'rift'; large_text = 'Rift Launcher' }
            }
            $label = 'menus'
        } else {
            $g = Find-Game $want
            if (-not $g) { return }
            $start = [DateTimeOffset]::new($Running[$want].Started).ToUnixTimeMilliseconds()
            $assets = [ordered]@{ large_image = 'rift'; large_text = 'Rift Launcher' }
            if ($g.coverRemote -match '^https://') {
                $assets = [ordered]@{ large_image = $g.coverRemote; large_text = (Discord-Clip $g.name 128); small_image = 'rift'; small_text = 'Rift Launcher' }
            }
            $activity = [ordered]@{
                details    = (Discord-Clip $g.name 128)
                state      = 'via Rift Launcher'
                timestamps = @{ start = $start }
                assets     = $assets
            }
            $label = $g.name
        }
        Discord-Send 1 ([ordered]@{ cmd = 'SET_ACTIVITY'; args = [ordered]@{ pid = $PID; activity = $activity }; nonce = [guid]::NewGuid().ToString() })
        if ($script:DiscordShown -ne $want) { Write-Log 'INFO' ('Statut Discord : ' + $label) }
        $script:DiscordShown = $want
    } catch {
        Write-Log 'WARN' ('Statut Discord impossible : ' + $_.Exception.Message)
        Discord-Close ''
    }
}

# Appelee dans la boucle : lit les reponses de Discord, suit menus / jeu,
# et retente toutes les 60 s si Discord a ete ouvert apres Rift Launcher.
function Discord-Tick {
    Discord-Drain
    $want = Discord-Wanted
    if ($want -eq $script:DiscordShown -and ($script:Discord -or -not $want)) { return }
    if ($script:Discord -or -not $want -or [DateTime]::UtcNow -gt $script:DiscordRetry) { Discord-Update }
}

# ---------------------------------------------------------------- manettes : bouton Home et capture
# Le code C# (RiftPad.cs) lit le bouton Xbox (XInput) et les boutons PS / Home / Share / Capture
# des manettes PlayStation et Switch, meme quand le jeu est au premier plan.
$script:PadReady = $false
$CaptureRoot = Join-Path ([Environment]::GetFolderPath('MyPictures')) 'Rift Launcher'

function Start-Pad {
    try {
        Add-Type -Path (Join-Path $Root 'RiftPad.cs') -ReferencedAssemblies 'System.Drawing', 'System.Windows.Forms'
        [RiftPad]::ShotExe = (Build-ShotExe)
        [RiftPad]::Start($CaptureRoot)
        $script:PadReady = $true
        Sync-Pad
        Write-Log 'INFO' ('Manettes : bouton Home et capture actifs (captures dans ' + $CaptureRoot + ')')
    } catch { Log-Error 'Manettes (bouton Home et capture)' $_ }
}

# RiftShot.exe (capture fidele DXGI) : compile une fois dans AppData, refait si le code change.
function Build-ShotExe {
    $src = Join-Path $Root 'RiftShot.cs'
    if (-not (Test-Path -LiteralPath $src)) { return '' }
    $binDir = Join-Path $DataDir 'bin'
    $exe = Join-Path $binDir 'RiftShot.exe'
    try {
        New-Item -ItemType Directory -Force -Path $binDir | Out-Null
        $stamp = Join-Path $binDir 'RiftShot.version'
        $want = [string](Get-Item -LiteralPath $src).LastWriteTimeUtc.Ticks + ' ' + (Get-Item -LiteralPath $src).Length
        $have = $(if (Test-Path -LiteralPath $stamp) { ([IO.File]::ReadAllText($stamp)).Trim() } else { '' })
        if (-not (Test-Path -LiteralPath $exe) -or $have -ne $want) {
            if (Test-Path -LiteralPath $exe) { Remove-Item -LiteralPath $exe -Force }
            Add-Type -Path $src -OutputAssembly $exe -OutputType WindowsApplication -ReferencedAssemblies 'System.Drawing'
            [IO.File]::WriteAllText($stamp, $want, $Utf8)
            Write-Log 'INFO' 'Capture fidele : RiftShot.exe compile'
        }
        return $exe
    } catch {
        Log-Error 'Compilation de RiftShot.exe (captures)' $_
        return ''
    }
}

# Donne au code C# le jeu en cours et les reglages.
function Sync-Pad {
    if (-not $script:PadReady) { return }
    try {
        $p = Get-Prefs
        [RiftPad]::HomeEnabled = [bool]$p.homeToggle
        [RiftPad]::CaptureEnabled = [bool]$p.captureButton
        $id = $(if ($Running.Count) { [string](@($Running.Keys | Sort-Object { $Running[$_].Started } -Descending)[0]) } else { '' })
        $g = $(if ($id) { Find-Game $id } else { $null })
        [RiftPad]::GamePid = $(if ($g) { [int]$Running[$id].Proc.Id } else { 0 })
        [RiftPad]::GameName = $(if ($g) { [string]$g.name } else { '' })
    } catch { Log-Error 'Manettes (synchro)' $_ }
}

function Pad-Tick {
    if (-not $script:PadReady) { return }
    foreach ($m in [RiftPad]::TakeLogs()) { Write-Log 'INFO' ('[manette] ' + $m) }
    if ([RiftPad]::WantWindow) { [RiftPad]::WantWindow = $false; Write-Log 'INFO' 'Bouton Home : reouverture de la fenetre'; Open-Window $Token }
}

function Capture-View {
    if (-not $script:PadReady) { return $null }
    return [ordered]@{ count = [RiftPad]::CaptureCount; last = [IO.Path]::GetFileName([RiftPad]::LastCapture) }
}

function Open-Captures([string]$id) {
    $dir = $CaptureRoot
    $g = $(if ($id) { Find-Game $id } else { $null })
    if ($g) {
        $name = ([string]$g.name).Trim()
        foreach ($c in [IO.Path]::GetInvalidFileNameChars()) { $name = $name.Replace([string]$c, '_') }
        $name = $name.TrimEnd('.')
        if ($name) { $dir = Join-Path $CaptureRoot $name }
    }
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Open-Explorer $dir
}

# ---------------------------------------------------------------- rapport de bug
function Get-OsName {
    try { return (Get-CimInstance Win32_OperatingSystem -ErrorAction Stop).Caption + ' (' + [Environment]::OSVersion.Version + ')' }
    catch { return [Environment]::OSVersion.VersionString }
}

function Find-Edge {
    foreach ($base in @(${env:ProgramFiles(x86)}, $env:ProgramFiles, $env:LOCALAPPDATA)) {
        if (-not $base) { continue }
        $e = Join-Path $base 'Microsoft\Edge\Application\msedge.exe'
        if (Test-Path -LiteralPath $e) { return $e }
    }
    return ''
}

# Cree "Rift Launcher - rapport <date>.zip" sur le Bureau : journaux, liste des jeux,
# reglages (sans la cle SteamGridDB) et infos systeme. C'est ce fichier qu'un ami envoie.
function New-Report {
    $stamp = (Get-Date).ToString('yyyy-MM-dd_HH-mm-ss')
    $tmp = Join-Path $env:TEMP ('RiftReport-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        Get-ChildItem -LiteralPath $LogDir -File | Where-Object { $_.Name -like 'rift-*.log' } |
            Sort-Object Name -Descending | Select-Object -First 5 | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $tmp }
        $setupLog = Join-Path $LogDir 'setup.log'
        if (Test-Path -LiteralPath $setupLog) { Copy-Item -LiteralPath $setupLog -Destination $tmp }
        if (Test-Path -LiteralPath $LibFile) { Copy-Item -LiteralPath $LibFile -Destination (Join-Path $tmp 'library.json') }

        $lines = New-Object System.Collections.Generic.List[string]
        $lines.Add('RAPPORT RIFT LAUNCHER')
        $lines.Add('Date            : ' + (Get-Date).ToString('yyyy-MM-dd HH:mm:ss'))
        $lines.Add('Version         : ' + $AppVersion)
        $lines.Add('Windows         : ' + (Get-OsName))
        $lines.Add('PowerShell      : ' + $PSVersionTable.PSVersion + ' / .NET ' + [Environment]::Version)
        $lines.Add('Langue          : ' + [Globalization.CultureInfo]::CurrentCulture.Name)
        $lines.Add('Edge            : ' + $(if (Find-Edge) { Find-Edge } else { 'INTROUVABLE' }))
        $lines.Add('Dossier appli   : ' + $Root)
        $lines.Add('Dossier donnees : ' + $DataDir)
        $lines.Add('Reglages        : ' + (((Get-Prefs).GetEnumerator() | ForEach-Object { $_.Key + '=' + $_.Value }) -join ', '))
        $lines.Add('Cle SteamGridDB : ' + $(if ((Load-Settings).sgdbKey) { 'oui (non incluse)' } else { 'non' }))
        $lines.Add('')
        $lines.Add('JEUX (' + $Games.Count + ')')
        foreach ($g in $Games) {
            $ok = { param($x, $type) if (-not $x) { 'non regle' } elseif (Test-Path -LiteralPath $x -PathType $type) { 'OK' } else { 'INTROUVABLE' } }
            $f = Get-Franchise $g
            $lines.Add('- ' + $g.name + ' | licence : ' + $(if ($f) { $f.key } else { 'aucune' }) +
                ' | exe : ' + (& $ok $g.exePath 'Leaf') + ' | saves : ' + (& $ok $g.saveDir 'Container') +
                ' | mods : ' + (& $ok $g.modsDir 'Container') + ' | sessions : ' + $g.sessions)
        }
        $lines.Add('')
        $lines.Add('FICHIERS DE L''APPLI')
        Get-ChildItem -LiteralPath $Root -Recurse -File | ForEach-Object {
            $lines.Add('  ' + $_.FullName.Substring($Root.Length).TrimStart('\') + '  (' + $_.Length + ' o)')
        }
        $lines.Add('')
        $lines.Add('SONS PERSO : ' + @(Get-ChildItem -LiteralPath $SoundDir -Recurse -File -ErrorAction SilentlyContinue).Count + ' fichier(s)')
        $lines.Add('JAQUETTES PERSO : ' + @(Get-ChildItem -LiteralPath $CoverDir -Recurse -File -ErrorAction SilentlyContinue).Count + ' fichier(s)')
        [IO.File]::WriteAllLines((Join-Path $tmp 'infos.txt'), $lines.ToArray(), $Utf8)

        $zip = Join-Path ([Environment]::GetFolderPath('Desktop')) ('Rift Launcher - rapport ' + $stamp + '.zip')
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [IO.Compression.ZipFile]::CreateFromDirectory($tmp, $zip)
        Write-Log 'INFO' ('Rapport cree : ' + $zip)
        Open-Explorer $zip -Select
        return @{ ok = $true; path = $zip }
    } catch {
        Log-Error 'Creation du rapport' $_
        return @{ error = 'Impossible de créer le rapport : ' + $_.Exception.Message }
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# ---------------------------------------------------------------- serveur HTTP
function Send-Bytes($ctx, [int]$status, [string]$type, [byte[]]$bytes) {
    $res = $ctx.Response
    $res.StatusCode = $status
    $res.ContentType = $type
    $res.Headers['Cache-Control'] = 'no-store'
    $res.ContentLength64 = $bytes.Length
    $res.OutputStream.Write($bytes, 0, $bytes.Length)
    $res.OutputStream.Close()
}

function Send-Json($ctx, $obj, [int]$status = 200) {
    $json = ConvertTo-Json -InputObject $obj -Depth 6 -Compress
    Send-Bytes $ctx $status 'application/json; charset=utf-8' ($Utf8.GetBytes($json))
}

$Static = @{
    '/'           = @('index.html', 'text/html; charset=utf-8')
    '/index.html' = @('index.html', 'text/html; charset=utf-8')
    '/style.css'  = @('style.css',  'text/css; charset=utf-8')
    '/app.js'     = @('app.js',     'text/javascript; charset=utf-8')
    '/i18n.js'    = @('i18n.js',    'text/javascript; charset=utf-8')
    '/i18n-dict.js' = @('i18n-dict.js', 'text/javascript; charset=utf-8')
    '/logo.svg'   = @('logo.svg',   'image/svg+xml')
}

function Handle-Request($ctx) {
    $req  = $ctx.Request
    $path = $req.Url.AbsolutePath

    # Refuse tout ce qui ne vient pas de localhost (protection DNS rebinding).
    if ($req.Url.Host -ne 'localhost') { Send-Json $ctx @{ error = 'forbidden' } 403; return }

    if ($Static.ContainsKey($path)) {
        $f = $Static[$path]
        Send-Bytes $ctx 200 $f[1] ([IO.File]::ReadAllBytes((Join-Path $WebDir $f[0])))
        return
    }
    if ($path -eq '/img' -or $path -eq '/snd') {
        $g = Find-Game ([string]$req.QueryString['id'])
        $asset = $null
        if ($g) {
            if ($path -eq '/img') { $asset = Resolve-Cover $g }
            else {
                $all = Resolve-Sounds $g
                $n = 0
                [void][int]::TryParse([string]$req.QueryString['n'], [ref]$n)
                if ($all -and $n -ge 0 -and $n -lt $all.paths.Count) { $asset = @{ path = $all.paths[$n] } }
            }
        }
        if (-not $asset) { Send-Json $ctx @{ error = 'not found' } 404; return }
        $ext = [IO.Path]::GetExtension($asset.path).ToLowerInvariant()
        $type = $Mime[$ext]
        if (-not $type) { $type = 'application/octet-stream' }
        Send-Bytes $ctx 200 $type ([IO.File]::ReadAllBytes($asset.path))
        return
    }
    if (-not $path.StartsWith('/api/')) { Send-Json $ctx @{ error = 'not found' } 404; return }

    # Jeton secret : un site web ne peut pas piloter le lanceur.
    $tok = $req.Headers['X-Token']
    if (-not $tok) { $tok = $req.QueryString['t'] }
    if ($tok -ne $Token -or $req.HttpMethod -ne 'POST') { Send-Json $ctx @{ error = 'forbidden' } 403; return }

    $script:LastSeen = [DateTime]::UtcNow
    $script:GotRequest = $true
    $body = $null
    $reader = New-Object System.IO.StreamReader($req.InputStream, $Utf8)
    $text = $reader.ReadToEnd(); $reader.Close()
    if ($text) { $body = $text | ConvertFrom-Json }
    $id = [string](Get-Field $body 'id')
    $api = $path.Substring(5)
    if ($api -ne 'library' -and $api -ne 'log') { Write-Log 'DEBUG' ('API ' + $api + $(if ($id) { ' (jeu ' + $id + ')' } else { '' })) }

    switch ($api) {
        'library' { Send-Json $ctx (Snapshot) }
        'save' {
            $name = ([string](Get-Field $body 'name')).Trim()
            if (-not $name) { Send-Json $ctx @{ error = 'Donne un nom au jeu.' }; return }
            $existing = $null
            if ($id) { $existing = Find-Game $id }
            if ($existing) {
                foreach ($f in $EditFields) { $existing[$f] = ([string](Get-Field $body $f)).Trim() }
            } else {
                $fresh = @{}
                foreach ($f in $EditFields) { $fresh[$f] = Get-Field $body $f }
                $Games.Add((New-Game $fresh $false))
            }
            Save-Library
            Start-RecompChecks $false
            Send-Json $ctx (Snapshot)
        }
        'delete' {
            if ($Running.ContainsKey($id)) { Send-Json $ctx @{ error = 'Ferme le jeu avant de le retirer.' }; return }
            $g = Find-Game $id
            if ($g) { [void]$Games.Remove($g); Save-Library }
            Send-Json $ctx (Snapshot)
        }
        'play'   { Send-Json $ctx (Start-Game $id) }
        'stop'   { Send-Json $ctx (Stop-Game $id) }
        'backup' {
            try {
                $b = Backup-Game $id
                if ($b.ok) { Write-Log 'INFO' ('Sauvegarde : ' + $b.path) } else { Write-Log 'WARN' ('Sauvegarde : ' + $b.error) }
                Send-Json $ctx $b
            } catch { Log-Error 'Sauvegarde' $_; Send-Json $ctx @{ error = $_.Exception.Message } }
        }
        'openBackups' {
            $g = Find-Game $id
            if ($g) {
                $dir = Join-Path $BackupRoot (Safe-Name $g.name)
                New-Item -ItemType Directory -Force -Path $dir | Out-Null
                Open-Explorer $dir
            }
            Send-Json $ctx @{ ok = $true }
        }
        'openFolder' {
            $g = Find-Game $id
            if ($g -and $g.exePath -and (Test-Path -LiteralPath $g.exePath)) {
                Open-Explorer $g.exePath -Select
            }
            Send-Json $ctx @{ ok = $true }
        }
        'openRepo' {
            $g = Find-Game $id
            if ($g -and $g.repoUrl -match '^https://') { Start-Process $g.repoUrl }
            Send-Json $ctx @{ ok = $true }
        }
        'mods'   { Send-Json $ctx (List-Mods $id) }
        'modSet' {
            Send-Json $ctx (Set-Mod $id ([string](Get-Field $body 'name')) ([bool](Get-Field $body 'enabled')))
        }
        'openMods' {
            $g = Find-Game $id
            if ($g -and $g.modsDir -and (Test-Path -LiteralPath $g.modsDir -PathType Container)) { Open-Explorer $g.modsDir }
            Send-Json $ctx @{ ok = $true }
        }
        'pick' { Send-Json $ctx (Show-Picker ([string](Get-Field $body 'kind'))) }
        'coverPick'     { Send-Json $ctx (Set-GameCoverFromFile $id) }
        'soundPick'     { Send-Json $ctx (Set-GameSoundFromFile $id) }
        'assetClear'    { Send-Json $ctx (Clear-GameAsset $id ([string](Get-Field $body 'kind'))) }
        'coverSearch'   { Send-Json $ctx (Search-Covers ([string](Get-Field $body 'term')) ([string](Get-Field $body 'sgdbId'))) }
        'coverDownload' { Send-Json $ctx (Download-Cover $id ([string](Get-Field $body 'url'))) }
        'settings' {
            $s = Load-Settings
            Send-Json $ctx @{
                appVersion = $AppVersion
                logDir = $LogDir
                prefs = (Get-Prefs)
                hasKey = [bool]$s.sgdbKey
                discordId = [string](Get-Field $s 'discordId')
                discordBuiltin = [bool]$DiscordClientId
                discordBuiltinId = $DiscordClientId
                discordConnected = [bool]$script:Discord
                coverDir = $CoverDir
                soundDir = $SoundDir
                franchises = @($Franchises | ForEach-Object { [ordered]@{ key = $_.key; label = $_.label } })
            }
        }
        'settingsSave' {
            $s = Load-Settings
            $k = Get-Field $body 'sgdbKey'
            if ($null -ne $k) { $k = ([string]$k).Trim(); $s | Add-Member -NotePropertyName sgdbKey -NotePropertyValue $k -Force }
            $dId = Get-Field $body 'discordId'
            if ($null -ne $dId) {
                $dId = ([string]$dId).Trim()
                if ($dId -and $dId -notmatch '^\d{15,22}$') { Send-Json $ctx @{ error = 'L''ID Discord est un nombre de 17 à 20 chiffres.' }; return }
                $s | Add-Member -NotePropertyName discordId -NotePropertyValue $dId -Force
            }
            Save-Settings $s
            if ($null -ne $dId) { Discord-Close ''; Discord-Update }
            Send-Json $ctx @{ ok = $true; hasKey = [bool]$s.sgdbKey }
        }
        'prefsSave' {
            $before = Get-Prefs
            $p = Save-Prefs (Get-Field $body 'prefs')
            if ($p.discord -ne $before.discord) { Discord-Update }
            if ($p.recompCheck -and -not $before.recompCheck) { Start-RecompChecks $false }
            Write-Log 'INFO' ('Reglages : ' + (($p.GetEnumerator() | ForEach-Object { $_.Key + '=' + $_.Value }) -join ', '))
            Send-Json $ctx @{ ok = $true; prefs = $p }
        }
        'log' {
            # Erreurs remontees par l'interface (JavaScript).
            $lvl = [string](Get-Field $body 'level'); if ($lvl -notin @('INFO', 'WARN', 'ERROR')) { $lvl = 'ERROR' }
            $msg = [string](Get-Field $body 'message')
            $stack = [string](Get-Field $body 'stack')
            if ($stack) { $msg += ' | ' + ($stack -replace "`r?`n", ' <- ') }
            if ($msg.Length -gt 2000) { $msg = $msg.Substring(0, 2000) + '...' }
            Write-Log $lvl ('[interface] ' + $msg)
            Send-Json $ctx @{ ok = $true }
        }
        'report'   { Send-Json $ctx (New-Report) }
        'version'  { Send-Json $ctx @{ version = $AppVersion } }
        'updateCheck'   { Send-Json $ctx (Check-UpdateNow) }
        'updateInstall' { Send-Json $ctx (Install-Update) }
        'recompCheck'   { Send-Json $ctx (Check-RecompsNow) }
        'toggleFocus' {
            $r = $(if ($script:PadReady) { [RiftPad]::Toggle() } else { 'indisponible' })
            Write-Log 'DEBUG' ('Bouton Home (interface) : ' + $r)
            Send-Json $ctx @{ ok = $true; result = $r }
        }
        'capture' {
            $f = $(if ($script:PadReady) { [RiftPad]::Capture() } else { '' })
            Send-Json $ctx @{ ok = [bool]$f; file = [IO.Path]::GetFileName($f) }
        }
        'openCaptures' { Open-Captures $id; Send-Json $ctx @{ ok = $true } }
        'fullscreen' {
            $r = $(if ($script:PadReady) { [RiftPad]::PressF11() } else { 'indisponible' })
            Write-Log 'INFO' ('Plein ecran (Big Picture, ' + $(if ([bool](Get-Field $body 'on')) { 'entree' } else { 'sortie' }) + ') : ' + $r)
            Send-Json $ctx @{ ok = $true; result = $r }
        }
        'recompDone'    { Send-Json $ctx (Set-RecompDone $id) }
        'recompOpen' {
            $g = Find-Game $id
            $v = $(if ($g) { Recomp-View $g } else { $null })
            if ($v -and $v.url -match '^https://github\.com/') { Start-Process $v.url }
            Send-Json $ctx @{ ok = [bool]$v }
        }
        'openLogs' { Open-Explorer $LogDir; Send-Json $ctx @{ ok = $true } }
        'openAssets' {
            if ([string](Get-Field $body 'kind') -eq 'sounds') { Open-Explorer $SoundDir } else { Open-Explorer $CoverDir }
            Send-Json $ctx @{ ok = $true }
        }
        'bye' {
            # Fenetre fermee : on s'arrete dans quelques secondes si elle ne revient pas.
            $script:LastSeen = [DateTime]::UtcNow.AddSeconds(5 - $IdleExitSeconds)
            Send-Json $ctx @{ ok = $true }
        }
        default { Send-Json $ctx @{ error = 'not found' } 404 }
    }
}

# ---------------------------------------------------------------- demarrage
$listener = New-Object System.Net.HttpListener
$listener.Prefixes.Add($BaseUrl)
try {
    $listener.Start()
} catch {
    # Deja lance : on rouvre juste la fenetre.
    Write-Log 'INFO' 'Deja lance : reouverture de la fenetre.'
    if (Test-Path -LiteralPath $TokenFile) { Open-Window ([IO.File]::ReadAllText($TokenFile).Trim()) }
    exit
}

Rotate-Logs
$script:DebugLog = [bool](Get-Prefs).debugLog
Write-Log 'INFO' ('===== Rift Launcher ' + $AppVersion + ' demarre =====')
Write-Log 'INFO' ('Windows : ' + (Get-OsName) + ' | PowerShell ' + $PSVersionTable.PSVersion + ' | .NET ' + [Environment]::Version + ' | ' + [Globalization.CultureInfo]::CurrentCulture.Name)
Write-Log 'INFO' ('Appli : ' + $Root + ' | Donnees : ' + $DataDir + ' | Edge : ' + $(if (Find-Edge) { 'trouve' } else { 'INTROUVABLE' }))

# Apres une mise a jour, on garde le jeton : la fenetre deja ouverte se reconnecte toute seule.
$Token = $null
if ($AfterUpdate -and (Test-Path -LiteralPath $TokenFile)) { $Token = ([IO.File]::ReadAllText($TokenFile)).Trim() }
if (-not $Token) { $Token = [guid]::NewGuid().ToString('N') }
[IO.File]::WriteAllText($TokenFile, $Token, $Utf8)
if ($AfterUpdate) { Write-Log 'INFO' ('Redemarrage apres mise a jour en ' + $AppVersion) }
Load-Library
Write-Readmes
Write-Log 'INFO' ('Bibliotheque : ' + $Games.Count + ' jeu(x) : ' + (($Games | ForEach-Object { $_.name }) -join ', '))
$LastSeen = [DateTime]::UtcNow
$script:StartedAt = [DateTime]::UtcNow
$script:GotRequest = $false
$WindowDeadline = $null
if ($AfterUpdate) { $WindowDeadline = [DateTime]::UtcNow.AddSeconds(20) } else { Open-Window $Token }
Start-UpdateCheck
Start-RecompChecks $false
Start-Pad

try {
    $pending = $listener.GetContextAsync()
    while ($true) {
        if ($pending.Wait(1000)) {
            $ctx = $pending.Result
            $pending = $listener.GetContextAsync()
            try { Handle-Request $ctx }
            catch {
                Log-Error ('Requete ' + $ctx.Request.Url.AbsolutePath) $_
                try { Send-Json $ctx @{ error = $_.Exception.Message } 500 } catch {}
            }
        }
        Update-Running
        Poll-UpdateCheck
        Poll-RecompChecks
        if ([DateTime]::UtcNow -gt $script:RecompNextCheck) { Start-RecompChecks $true }
        Discord-Tick
        Pad-Tick
        if ($script:QuitForUpdate) { break }
        # Apres une mise a jour : si l'ancienne fenetre ne s'est pas reconnectee, on en ouvre une.
        if ($WindowDeadline -and -not $script:GotRequest -and [DateTime]::UtcNow -gt $WindowDeadline) {
            $WindowDeadline = $null
            Open-Window $Token
        }
        # S'arrete quand la fenetre est fermee et qu'aucun jeu ne tourne.
        if ($Running.Count -eq 0 -and ([DateTime]::UtcNow - $LastSeen).TotalSeconds -gt $IdleExitSeconds) { break }
    }
} finally {
    foreach ($id in @($Running.Keys)) { Add-Time $id }
    Save-Library
    Discord-Close ''
    $listener.Stop()
    Write-Log 'INFO' 'Arret de Rift Launcher.'
}
