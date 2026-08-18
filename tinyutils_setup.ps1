param([switch]$uninstall)

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

function Update-Step($index, $status) {
    if ($status -eq "active") {
        $script:steps[$index].Text = "[ > ] " + $script:stepData[$index][0]
        $script:steps[$index].ForeColor = [Drawing.Color]::FromArgb(255, 200, 0)
    } elseif ($status -eq "done") {
        $script:steps[$index].Text = "[ OK ] " + $script:stepData[$index][0]
        $script:steps[$index].ForeColor = [Drawing.Color]::FromArgb(50, 220, 100)
    } elseif ($status -eq "error") {
        $script:steps[$index].Text = "[ X ] " + $script:stepData[$index][0]
        $script:steps[$index].ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
    }
    $script:form.Refresh()
}

function Set-Progress($value) {
    $script:progressBar.Value = $value
    $script:form.Refresh()
}

function Show-ChoiceScreen {
    $f = New-Object System.Windows.Forms.Form
    $f.Text = "Tiny Utils"
    $f.ClientSize = New-Object System.Drawing.Size(360, 220)
    $f.StartPosition = "CenterScreen"
    $f.FormBorderStyle = "FixedDialog"
    $f.MaximizeBox = $false
    $f.MinimizeBox = $false
    $f.BackColor = [Drawing.Color]::FromArgb(30, 30, 35)

    $t = New-Object System.Windows.Forms.Label
    $t.Text = "TINY UTILS"
    $t.Font = New-Object System.Drawing.Font("Segoe UI", 18, [Drawing.FontStyle]::Bold)
    $t.ForeColor = [Drawing.Color]::FromArgb(0, 200, 255)
    $t.AutoSize = $true
    $t.Location = New-Object System.Drawing.Point(90, 25)
    $f.Controls.Add($t)

    $s = New-Object System.Windows.Forms.Label
    $s.Text = "What do you want to do?"
    $s.Font = New-Object System.Drawing.Font("Segoe UI", 10)
    $s.ForeColor = [Drawing.Color]::FromArgb(150, 150, 160)
    $s.AutoSize = $true
    $s.Location = New-Object System.Drawing.Point(105, 60)
    $f.Controls.Add($s)

    $iBtn = New-Object System.Windows.Forms.Button
    $iBtn.Text = "Install Mod"
    $iBtn.Font = New-Object System.Drawing.Font("Segoe UI", 11, [Drawing.FontStyle]::Bold)
    $iBtn.Size = New-Object System.Drawing.Size(140, 50)
    $iBtn.Location = New-Object System.Drawing.Point(30, 100)
    $iBtn.BackColor = [Drawing.Color]::FromArgb(0, 120, 180)
    $iBtn.ForeColor = [Drawing.Color]::White
    $iBtn.FlatStyle = "Flat"
    $iBtn.Add_Click({ $f.Tag = "install"; $f.Close() })
    $f.Controls.Add($iBtn)

    $uBtn = New-Object System.Windows.Forms.Button
    $uBtn.Text = "Uninstall Mod"
    $uBtn.Font = New-Object System.Drawing.Font("Segoe UI", 11, [Drawing.FontStyle]::Bold)
    $uBtn.Size = New-Object System.Drawing.Size(140, 50)
    $uBtn.Location = New-Object System.Drawing.Point(190, 100)
    $uBtn.BackColor = [Drawing.Color]::FromArgb(160, 40, 40)
    $uBtn.ForeColor = [Drawing.Color]::White
    $uBtn.FlatStyle = "Flat"
    $uBtn.Add_Click({ $f.Tag = "uninstall"; $f.Close() })
    $f.Controls.Add($uBtn)

    $f.ShowDialog() | Out-Null
    return $f.Tag
}

function Build-MainForm($mode) {
    $script:form = New-Object System.Windows.Forms.Form
    $script:form.Text = "Tiny Utils - " + $(if ($mode -eq "install") { "Installer" } else { "Uninstaller" })
    $script:form.ClientSize = New-Object System.Drawing.Size(520, 520)
    $script:form.StartPosition = "CenterScreen"
    $script:form.FormBorderStyle = "FixedDialog"
    $script:form.MaximizeBox = $false
    $script:form.MinimizeBox = $false
    $script:form.BackColor = [Drawing.Color]::FromArgb(30, 30, 35)

    $accent = if ($mode -eq "install") { [Drawing.Color]::FromArgb(0, 200, 255) } else { [Drawing.Color]::FromArgb(255, 100, 100) }

    $tLabel = New-Object System.Windows.Forms.Label
    $tLabel.Text = "TINY UTILS " + $(if ($mode -eq "install") { "INSTALLER" } else { "UNINSTALLER" })
    $tLabel.Font = New-Object System.Drawing.Font("Segoe UI", 16, [Drawing.FontStyle]::Bold)
    $tLabel.ForeColor = $accent
    $tLabel.AutoSize = $true
    $tLabel.Location = New-Object System.Drawing.Point(30, 20)
    $script:form.Controls.Add($tLabel)

    $sLabel = New-Object System.Windows.Forms.Label
    $sLabel.Text = $(if ($mode -eq "install") { "Installing mod for Tiny Glade" } else { "Removing mod from Tiny Glade" })
    $sLabel.Font = New-Object System.Drawing.Font("Segoe UI", 9)
    $sLabel.ForeColor = [Drawing.Color]::FromArgb(150, 150, 160)
    $sLabel.AutoSize = $true
    $sLabel.Location = New-Object System.Drawing.Point(30, 50)
    $script:form.Controls.Add($sLabel)

    $script:progressBar = New-Object System.Windows.Forms.ProgressBar
    $script:progressBar.Location = New-Object System.Drawing.Point(30, 80)
    $script:progressBar.Size = New-Object System.Drawing.Size(440, 6)
    $script:progressBar.Style = "Continuous"
    $script:progressBar.ForeColor = $accent
    $script:progressBar.Value = 0
    $script:form.Controls.Add($script:progressBar)

    if ($mode -eq "install") {
        $script:stepData = @(
            @("Detecting game folder...", ""),
            @("Removing old mod...", ""),
            @("Downloading dxgi.dll from GitHub...", ""),
            @("Verifying download...", ""),
            @("Installing mod...", "")
        )
    } else {
        $script:stepData = @(
            @("Detecting game folder...", ""),
            @("Checking for dxgi.dll...", ""),
            @("Closing game if running...", ""),
            @("Deleting dxgi.dll...", ""),
            @("Verifying removal...", "")
        )
    }

    $script:steps = @()
    $stepFont = New-Object System.Drawing.Font("Segoe UI", 10)
    $stepY = 110
    foreach ($step in $script:stepData) {
        $label = New-Object System.Windows.Forms.Label
        $label.Text = "[ . ] " + $step[0]
        $label.Font = $stepFont
        $label.ForeColor = [Drawing.Color]::FromArgb(100, 100, 110)
        $label.AutoSize = $true
        $label.Location = New-Object System.Drawing.Point(30, $stepY)
        $script:form.Controls.Add($label)
        $script:steps += $label
        $stepY += 35
    }

    $script:statusLabel = New-Object System.Windows.Forms.Label
    $script:statusLabel.Text = ""
    $script:statusLabel.Font = New-Object System.Drawing.Font("Segoe UI", 9)
    $script:statusLabel.ForeColor = [Drawing.Color]::FromArgb(120, 120, 130)
    $script:statusLabel.AutoSize = $true
    $script:statusLabel.Location = New-Object System.Drawing.Point(30, 300)
    $script:form.Controls.Add($script:statusLabel)

    $script:resultLabel = New-Object System.Windows.Forms.Label
    $script:resultLabel.Text = ""
    $script:resultLabel.Font = New-Object System.Drawing.Font("Segoe UI", 11, [Drawing.FontStyle]::Bold)
    $script:resultLabel.AutoSize = $true
    $script:resultLabel.Location = New-Object System.Drawing.Point(30, 335)
    $script:form.Controls.Add($script:resultLabel)

    $script:instructionsLabel = New-Object System.Windows.Forms.Label
    $script:instructionsLabel.Text = ""
    $script:instructionsLabel.Font = New-Object System.Drawing.Font("Segoe UI", 9)
    $script:instructionsLabel.ForeColor = [Drawing.Color]::FromArgb(0, 180, 255)
    $script:instructionsLabel.AutoSize = $true
    $script:instructionsLabel.Location = New-Object System.Drawing.Point(30, 370)
    $script:form.Controls.Add($script:instructionsLabel)

    $script:closeButton = New-Object System.Windows.Forms.Button
    $script:closeButton.Text = "Close"
    $script:closeButton.Font = New-Object System.Drawing.Font("Segoe UI", 9)
    $script:closeButton.Size = New-Object System.Drawing.Size(100, 32)
    $script:closeButton.Location = New-Object System.Drawing.Point(370, 450)
    $script:closeButton.BackColor = [Drawing.Color]::FromArgb(45, 45, 50)
    $script:closeButton.ForeColor = [Drawing.Color]::FromArgb(200, 200, 210)
    $script:closeButton.FlatStyle = "Flat"
    $script:closeButton.Visible = $false
    $script:closeButton.Add_Click({ $script:form.Close() })
    $script:form.Controls.Add($script:closeButton)
}

function Run-Install {
    $script:success = $false
    $script:gameDir = (Get-Location).Path

    $script:form.Add_Shown({
        $script:form.Activate()

        Update-Step 0 "active"
        Set-Progress 5
        Start-Sleep -Milliseconds 400
        if (Test-Path (Join-Path (Get-Location).Path "tiny-glade.exe")) {
            $script:gameDir = (Get-Location).Path
        } elseif (Test-Path (Join-Path (Get-Location).Path "dxgi.dll")) {
            $script:gameDir = (Get-Location).Path
        }
        $script:statusLabel.Text = "Folder: $script:gameDir"
        Update-Step 0 "done"
        Set-Progress 15

        Update-Step 1 "active"
        Start-Sleep -Milliseconds 300
        $dllPath = Join-Path $script:gameDir "dxgi.dll"
        if (Test-Path $dllPath) { try { Remove-Item $dllPath -Force } catch {} }
        Update-Step 1 "done"
        Set-Progress 25

        Update-Step 2 "active"
        Set-Progress 30
        $url = "https://github.com/P1kaCat/Tiny-Utils/releases/latest/download/dxgi.dll"
        try {
            $client = New-Object System.Net.WebClient
            $client.DownloadFile($url, $dllPath)
            Update-Step 2 "done"
            Set-Progress 70
        } catch {
            Update-Step 2 "error"
            $script:statusLabel.Text = "Error: $($_.Exception.Message)"
            $script:resultLabel.Text = "Installation failed"
            $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
            $script:closeButton.Visible = $true
            return
        }

        Update-Step 3 "active"
        Start-Sleep -Milliseconds 300
        if (-not (Test-Path $dllPath)) {
            Update-Step 3 "error"
            $script:resultLabel.Text = "Download failed - file missing"
            $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
            $script:closeButton.Visible = $true
            return
        }
        $fileSize = (Get-Item $dllPath).Length
        $script:statusLabel.Text = "Size: $fileSize bytes"
        if ($fileSize -lt 50000) {
            Remove-Item $dllPath -Force
            Update-Step 3 "error"
            $script:resultLabel.Text = "File too small - invalid download"
            $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
            $script:closeButton.Visible = $true
            return
        }
        Update-Step 3 "done"
        Set-Progress 85

        Update-Step 4 "active"
        Start-Sleep -Milliseconds 300
        Update-Step 4 "done"
        Set-Progress 100

        $script:success = $true
        $script:resultLabel.Text = "Tiny Utils installed successfully!"
        $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(50, 220, 100)
        $script:instructionsLabel.Text = "1. Run tiny-glade.exe`n2. Menu opens in-game`n`nTo uninstall: run this again and pick Uninstall"
        $script:closeButton.Visible = $true
    })
}

function Run-Uninstall {
    $script:success = $false
    $script:gameDir = (Get-Location).Path

    $script:form.Add_Shown({
        $script:form.Activate()

        Update-Step 0 "active"
        Set-Progress 5
        Start-Sleep -Milliseconds 400
        if (Test-Path (Join-Path (Get-Location).Path "tiny-glade.exe")) {
            $script:gameDir = (Get-Location).Path
        } elseif (Test-Path (Join-Path (Get-Location).Path "dxgi.dll")) {
            $script:gameDir = (Get-Location).Path
        }
        $script:statusLabel.Text = "Folder: $script:gameDir"
        Update-Step 0 "done"
        Set-Progress 15

        Update-Step 1 "active"
        Start-Sleep -Milliseconds 300
        $dllPath = Join-Path $script:gameDir "dxgi.dll"
        if (-not (Test-Path $dllPath)) {
            Update-Step 1 "done"
            Set-Progress 100
            $script:resultLabel.Text = "Mod is not installed"
            $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(150, 150, 160)
            $script:instructionsLabel.Text = "dxgi.dll was not found. Game is already vanilla."
            $script:closeButton.Visible = $true
            return
        }
        $script:statusLabel.Text = "dxgi.dll found ($((Get-Item $dllPath).Length) bytes)"
        Update-Step 1 "done"
        Set-Progress 30

        Update-Step 2 "active"
        Set-Progress 35
        $gameProc = Get-Process -Name "tiny-glade" -ErrorAction SilentlyContinue
        if ($gameProc) {
            $script:statusLabel.Text = "Tiny Glade is running. Closing it..."
            Start-Process "taskkill" -ArgumentList "/F", "/IM", "tiny-glade.exe" -NoNewWindow -Wait
            Start-Sleep -Seconds 1
            $gameProc = Get-Process -Name "tiny-glade" -ErrorAction SilentlyContinue
            if ($gameProc) {
                Update-Step 2 "error"
                $script:resultLabel.Text = "Cannot close Tiny Glade"
                $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
                $script:instructionsLabel.Text = "Close Tiny Glade manually (Task Manager) then run again."
                $script:closeButton.Visible = $true
                return
            }
        }
        Update-Step 2 "done"
        Set-Progress 50

        Update-Step 3 "active"
        Set-Progress 55
        $deleted = $false
        try { Remove-Item $dllPath -Force -ErrorAction Stop; $deleted = $true } catch {}
        if (-not $deleted) {
            try { attrib -R -S -H $dllPath 2>$null; Remove-Item $dllPath -Force -ErrorAction Stop; $deleted = $true } catch {}
        }
        if (-not $deleted) {
            try { Start-Process cmd -ArgumentList "/c", "del /F /Q `"$dllPath`"" -Verb RunAs -Wait -ErrorAction Stop; $deleted = $true } catch {}
        }
        if (-not $deleted) {
            Update-Step 3 "error"
            $script:resultLabel.Text = "Could not delete dxgi.dll"
            $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
            $script:instructionsLabel.Text = "Delete dxgi.dll manually from:`n$script:gameDir"
            $script:closeButton.Visible = $true
            return
        }
        Update-Step 3 "done"
        Set-Progress 85

        Update-Step 4 "active"
        Start-Sleep -Milliseconds 300
        if (Test-Path $dllPath) {
            Update-Step 4 "error"
            $script:resultLabel.Text = "dxgi.dll still present"
            $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(255, 80, 80)
            $script:instructionsLabel.Text = "Delete dxgi.dll manually from:`n$script:gameDir"
            $script:closeButton.Visible = $true
            return
        }
        Update-Step 4 "done"
        Set-Progress 100

        $script:success = $true
        $script:resultLabel.Text = "Tiny Utils uninstalled!"
        $script:resultLabel.ForeColor = [Drawing.Color]::FromArgb(50, 220, 100)
        $script:instructionsLabel.Text = "Game is back to vanilla. Run this again and pick Install to reinstall."
        $script:closeButton.Visible = $true
    })
}

if ($uninstall) {
    $mode = "uninstall"
} else {
    $mode = Show-ChoiceScreen
    if (-not $mode) { exit 0 }
}

Build-MainForm $mode
if ($mode -eq "install") { Run-Install } else { Run-Uninstall }

$script:form.ShowDialog() | Out-Null
if ($script:success) { exit 0 } else { exit 1 }
