& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    $versionPattern = '(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'

    function Read-LazelChoice([string]$Question, [bool]$DefaultYes) {
        while ($true) {
            try {
                Write-Host "$Question " -NoNewline
                $answer = $Host.UI.ReadLine()
            } catch [System.Management.Automation.PSInvalidOperationException], [System.Management.Automation.Host.HostException] {
                return $null
            }
            if ($null -eq $answer -or $answer.Equals([string][char]26, [StringComparison]::Ordinal)) { return $false }
            switch -Regex ($answer.Trim()) {
                '^$' { return $DefaultYes }
                '^(y|yes)$' { return $true }
                '^(n|no)$' { return $false }
                default { Write-Host 'Please enter y or n.' }
            }
        }
    }

    function Get-LazelVersion([string]$Path) {
        try {
            $reported = (& $Path --version 2>$null) -join "`n"
            $parsed = $null
            if ($LASTEXITCODE -eq 0 -and $reported -cmatch "\Alazel $versionPattern\z" -and
                [version]::TryParse($reported.Substring(6), [ref]$parsed)) {
                return $parsed
            }
        } catch {
            return $null
        }
    }

    $interactive = $Host.Name -eq 'ConsoleHost' -and !$env:CI -and $env:LAZEL_INSTALL_NONINTERACTIVE -ne '1' -and
        ![Console]::IsInputRedirected -and ![Console]::IsOutputRedirected -and
        !([Environment]::GetCommandLineArgs() -match '^[-/]noni')
    Write-Host ''
    if ($interactive -and $env:TERM -ne 'dumb' -and !$env:NO_COLOR) {
        Write-Host '  Laz' -ForegroundColor Cyan -NoNewline
        Start-Sleep -Milliseconds 80
        Write-Host 'el installer' -ForegroundColor Cyan
    } else {
        Write-Host '  Lazel installer'
    }
    Write-Host '  Run build, test, and lint tasks in a mixed-language monorepo.'
    Write-Host ''
    Write-Host '[Check] Looking up the platform and latest release.'
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
    foreach ($tool in @('Invoke-RestMethod', 'Invoke-WebRequest', 'Get-FileHash', 'Expand-Archive')) {
        if (!(Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Lazel requires the $tool command to install." }
    }

    $installDir = Join-Path $env:USERPROFILE '.local\bin'
    $destination = Join-Path $installDir 'lazel.exe'
    $existing = Get-Item -LiteralPath $destination -Force -ErrorAction SilentlyContinue
    if ($existing -and ($existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint))) {
        throw "Cannot install Lazel because '$destination' is a directory or link."
    }

    $workDir = $null
    $keepBackup = $false
    $securityProtocol = [Net.ServicePointManager]::SecurityProtocol

    try {
        if ($PSVersionTable.PSEdition -eq 'Desktop') {
            [Net.ServicePointManager]::SecurityProtocol = $securityProtocol -bor [Net.SecurityProtocolType]::Tls12
        }

        $releaseBase = 'https://github.com/rohan-prasen/lazel/releases'
        try {
            $release = Invoke-RestMethod -UseBasicParsing -TimeoutSec 30 -Uri 'https://api.github.com/repos/rohan-prasen/lazel/releases/latest'
        } catch {
            throw "Could not find the latest Lazel release. Check your connection and $releaseBase/latest. $($_.Exception.Message)"
        }
        $tag = $release.tag_name
        $available = $null
        if ($tag -isnot [string] -or $tag -cnotmatch "\Av$versionPattern\z" -or
            ![version]::TryParse($tag.Substring(1), [ref]$available)) {
            throw "The latest Lazel release has an invalid release tag. See $releaseBase/latest."
        }
        $releaseUrl = "$releaseBase/tag/$tag"
        Write-Host "Platform           $target"
        Write-Host "Destination        $destination"
        Write-Host "Available release  $tag"
        $active = Get-Command lazel -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($active -and $active.Source -ne $destination) {
            Write-Host "First on PATH      $($active.Source)"
        }
        $action = 'Install'
        if ($existing) {
            $action = 'Reinstall'
            $installed = Get-LazelVersion $destination
            if ($installed) {
                Write-Host "Installed version  v$installed"
                if ($installed -gt $available) {
                    Write-Host 'Keeping the newer installed version.'
                    return
                }
                if ($installed -lt $available) { $action = 'Update' }
            } else {
                Write-Host 'Installed version  unreadable (Reinstall replaces this copy)'
            }
        }
        Write-Host "Action             $action"
        if ($interactive) {
            Write-Host ''
            $choice = Read-LazelChoice "$action Lazel? [Y/n]" $true
            if ($null -eq $choice) {
                $interactive = $false
            } elseif (!$choice) {
                Write-Host 'Installation cancelled.'
                return
            }
        }

        Write-Host ''
        Write-Host '[Stage] Preparing the install directory.'
        [void][IO.Directory]::CreateDirectory($installDir)
        $workDir = Join-Path $installDir ('.lazel.' + [guid]::NewGuid().ToString('N'))
        [void][IO.Directory]::CreateDirectory($workDir)
        $archive = "lazel-$target.zip"
        $archivePath = Join-Path $workDir $archive
        $checksumPath = "$archivePath.sha256"
        Write-Host "[Download] Fetching $archive and its checksum."
        try {
            Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri "$releaseBase/download/$tag/$archive" -OutFile $archivePath
            Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri "$releaseBase/download/$tag/$archive.sha256" -OutFile $checksumPath
        } catch {
            throw "Could not download the Windows release assets. Check your connection and confirm that $archive and its checksum are listed at $releaseUrl. $($_.Exception.Message)"
        }

        Write-Host '[Verify] Checking the SHA-256 checksum.'
        $checksum = (Get-Content -LiteralPath $checksumPath -Raw) -split '\s+', 2
        if ($checksum[0] -notmatch '^[a-f0-9]{64}$') {
            throw 'The release has an invalid SHA-256 checksum file.'
        }
        if ((Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash -ne $checksum[0]) {
            throw 'Lazel download failed SHA-256 verification.'
        }

        Write-Host "[Stage] Extracting and checking Lazel $tag."
        $expanded = Join-Path $workDir 'expanded'
        try {
            Expand-Archive -LiteralPath $archivePath -DestinationPath $expanded
        } catch {
            throw "Could not extract lazel.exe from $archive. $($_.Exception.Message)"
        }
        $binary = Join-Path $expanded 'lazel.exe'
        if (!(Test-Path -LiteralPath $binary -PathType Leaf) -or (Get-Item -LiteralPath $binary).Length -eq 0) {
            throw "Could not extract lazel.exe from $archive."
        }
        if (((Get-Item -LiteralPath $binary -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            (Get-LazelVersion $binary) -ne $available) {
            throw "Expected lazel $available in the release archive. The staged executable reported a different or unreadable version."
        }
        $existing = Get-Item -LiteralPath $destination -Force -ErrorAction SilentlyContinue
        if ($existing -and ($existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint))) {
            throw "Cannot replace '$destination' because it changed to a directory or link."
        }

        Write-Host '[Install] Placing the verified executable.'
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
        if ($workDir -and !$keepBackup) {
            Remove-Item -LiteralPath $workDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    $outcome = switch ($action) {
        'Install' { 'Installed' }
        'Update' { 'Updated' }
        'Reinstall' { 'Reinstalled' }
    }
    Write-Host ''
    Write-Host "$outcome Lazel $tag at $destination"
    $active = Get-Command lazel -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (!$active -or $active.Source -ne $destination) {
        Write-Host 'To use Lazel in this shell, run:'
        Write-Host '  $env:Path = "$env:USERPROFILE\.local\bin;$env:Path"'
    }
    Write-Host 'Verify your installation with: lazel --version'
    if ($action -eq 'Install') {
        Write-Host 'Get started with: lazel --help'
        Write-Host 'In a workspace with lazel.toml, try a task such as: lazel run app:build'
    } else {
        Write-Host "Release notes: $releaseUrl"
    }
    Write-Host 'Update later with: lazel update'
    if ($interactive) {
        Write-Host ''
        if (Read-LazelChoice 'Show Lazel help now? [y/N]' $false) {
            try {
                & $destination --help
                if ($LASTEXITCODE -ne 0) { throw 'Lazel help returned an error.' }
            } catch {
                Write-Warning 'Could not show help. Try lazel --help in your shell.'
            }
        }
    }
}
