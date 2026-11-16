& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    if ($env:OS -ne 'Windows_NT' -or $PSVersionTable.PSVersion -lt [version]'5.1') {
        throw 'This installer requires Windows and PowerShell 5.1 or later.'
    }

    $processor = Get-CimInstance -ClassName Win32_Processor -Property Architecture, AddressWidth |
        Select-Object -First 1
    $target = switch ($processor.Architecture) {
        9 { 'x86_64-pc-windows-msvc' }
        12 { 'aarch64-pc-windows-msvc' }
    }
    if ($processor.AddressWidth -ne 64 -or !$target) {
        throw 'Lazel requires 64-bit Windows on x86-64 or ARM64.'
    }
    if (!$env:USERPROFILE) {
        throw 'Could not find your Windows profile. Set USERPROFILE and run the installer again.'
    }

    $installDir = Join-Path $env:USERPROFILE '.local\bin'
    $destination = Join-Path $installDir 'lazel.exe'
    if (Test-Path -LiteralPath $destination) {
        $existing = Get-Item -LiteralPath $destination -Force
        if ($existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Cannot install Lazel because '$destination' is a directory or link."
        }
    }

    [void][IO.Directory]::CreateDirectory($installDir)
    $workDir = Join-Path $installDir ('.lazel.' + [guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($workDir)
    $keepBackup = $false
    $securityProtocol = [Net.ServicePointManager]::SecurityProtocol

    try {
        if ($PSVersionTable.PSEdition -eq 'Desktop') {
            [Net.ServicePointManager]::SecurityProtocol = $securityProtocol -bor [Net.SecurityProtocolType]::Tls12
        }

        $archive = "lazel-$target.zip"
        $archivePath = Join-Path $workDir $archive
        $checksumPath = "$archivePath.sha256"
        $releaseUrl = 'https://github.com/rohan-prasen/lazel/releases/latest'
        Write-Host "Downloading Lazel for $target"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/download/$archive" -OutFile $archivePath
            Invoke-WebRequest -UseBasicParsing -Uri "$releaseUrl/download/$archive.sha256" -OutFile $checksumPath
        } catch {
            throw "Could not download the Windows release assets. Check your connection and confirm that $archive and its checksum are listed at $releaseUrl. $($_.Exception.Message)"
        }

        Write-Host 'Verifying SHA-256 checksum'
        $checksum = (Get-Content -LiteralPath $checksumPath -Raw) -split '\s+', 2
        if ($checksum[0] -notmatch '^[a-f0-9]{64}$') {
            throw 'The release has an invalid SHA-256 checksum file.'
        }
        if ((Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash -ne $checksum[0]) {
            throw 'Lazel download failed SHA-256 verification.'
        }

        Write-Host "Installing Lazel to $installDir"
        $expanded = Join-Path $workDir 'expanded'
        Expand-Archive -LiteralPath $archivePath -DestinationPath $expanded
        $binary = Join-Path $expanded 'lazel.exe'
        if (!(Test-Path -LiteralPath $binary -PathType Leaf) -or (Get-Item -LiteralPath $binary).Length -eq 0) {
            throw "Could not extract lazel.exe from $archive."
        }

        $backup = Join-Path $workDir 'previous.exe'
        try {
            if ([IO.File]::Exists($destination)) {
                [IO.File]::Replace($binary, $destination, $backup)
            } else {
                [IO.File]::Move($binary, $destination)
            }
        } catch {
            $replacementError = $_.Exception.Message
            if ([IO.File]::Exists($backup)) {
                $keepBackup = $true
                if (![IO.File]::Exists($destination)) {
                    try {
                        [IO.File]::Move($backup, $destination)
                        $keepBackup = $false
                    } catch {
                        throw "Could not restore Lazel at '$destination'. Your previous executable is saved at '$backup'. $($_.Exception.Message)"
                    }
                }
                if ($keepBackup) {
                    throw "Could not replace '$destination'. Your previous executable is saved at '$backup'. $replacementError"
                }
            }
            throw "Could not install to '$destination'. Close any running Lazel processes, check the file permissions, and retry. $replacementError"
        }
    } finally {
        [Net.ServicePointManager]::SecurityProtocol = $securityProtocol
        if (!$keepBackup) {
            Remove-Item -LiteralPath $workDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    Write-Host "Installed Lazel to $destination"
    if (($env:Path -split ';') -notcontains $installDir) {
        Write-Host 'To use Lazel in this shell, run:'
        Write-Host '  $env:Path = "$env:USERPROFILE\.local\bin;$env:Path"'
    }
    Write-Host 'Verify your installation with: lazel --version'
}
