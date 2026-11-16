param([Parameter(Mandatory)][string]$ArchivePath)

$ErrorActionPreference = 'Stop'
$fixtureArchive = Get-Item -LiteralPath $ArchivePath
$installer = Join-Path $PSScriptRoot '../script/install.ps1'
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('Lazel installer ' + [guid]::NewGuid())
$installDir = Join-Path $fixture '.local\bin'
$destination = Join-Path $installDir 'lazel.exe'
$originalProfile = $env:USERPROFILE
$originalPath = $env:Path
$corruptChecksum = $false

function Invoke-WebRequest {
    param([switch]$UseBasicParsing, [uri]$Uri, [string]$OutFile)

    $name = [IO.Path]::GetFileName($Uri.AbsolutePath)
    if ($name -eq $fixtureArchive.Name) {
        Copy-Item -LiteralPath $fixtureArchive.FullName -Destination $OutFile
    } elseif ($name -eq "$($fixtureArchive.Name).sha256") {
        if ($corruptChecksum) {
            ('0' * 64) | Set-Content -LiteralPath $OutFile -Encoding ascii
        } else {
            Copy-Item -LiteralPath "$($fixtureArchive.FullName).sha256" -Destination $OutFile
        }
    } else {
        throw "Unexpected installer asset: $name"
    }
}

try {
    $env:USERPROFILE = $fixture
    $scriptText = Get-Content -LiteralPath $installer -Raw
    $output = $scriptText | Invoke-Expression 6>&1 | Out-String
    if ($output -notmatch '\$env:Path\s*=' -or $output -notmatch 'lazel --version') {
        throw "Missing installer PATH or verification instructions. $output"
    }
    $version = & $destination --version
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^lazel \d+\.\d+\.\d+$') {
        throw "Unexpected installed binary version: $version"
    }
    $installedHash = (Get-FileHash -LiteralPath $destination).Hash

    [IO.File]::WriteAllText($destination, 'old executable')
    $env:Path = "$installDir;$originalPath"
    $output = $scriptText | Invoke-Expression 6>&1 | Out-String
    if ($output -match '\$env:Path\s*=' -or (Get-FileHash -LiteralPath $destination).Hash -ne $installedHash) {
        throw "Upgrade failed or printed unnecessary PATH instructions. $output"
    }

    $corruptChecksum = $true
    try {
        $scriptText | Invoke-Expression
        throw 'The installer accepted a corrupt checksum.'
    } catch {
        if ($_.Exception.Message -notmatch 'failed SHA-256 verification') {
            throw
        }
    }
    if ((Get-FileHash -LiteralPath $destination).Hash -ne $installedHash) {
        throw 'Checksum failure changed the existing executable.'
    }
    if (Get-ChildItem -LiteralPath $installDir -Force -Filter '.lazel.*') {
        throw 'The installer left temporary files behind.'
    }
    Write-Host 'Windows installer checks passed.'
} finally {
    $env:USERPROFILE = $originalProfile
    $env:Path = $originalPath
    Remove-Item -LiteralPath $fixture -Recurse -Force -ErrorAction SilentlyContinue
}
