# Pack EpubReader project as zip
$ErrorActionPreference = 'Stop'

$source = 'E:\epubreader'
$zipPath = 'E:\epubreader.zip'

# Remove any existing partial zip
if (Test-Path $zipPath) {
    Remove-Item $zipPath -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Milliseconds 200

# Exclusion rules for source tree
$excludedDirPrefixes = @(
    'node_modules'
    'target\debug'
    'target\release\deps'
    'target\release\build'
    'target\release\.fingerprint'
    'target\release\incremental'
    'target\release\examples'
    'target\release\data'
    'gen'
    'dist'
    'nsis\x64'
)

# Specific files to exclude from source tree
$excludedFiles = @(
    'pack.ps1'
    'test.ps1'
    '.DS_Store'
    'Thumbs.db'
)

# Pre-built binaries that should be placed at zip root (instead of in src-tauri/target/...)
$rootBinaries = @(
    @{ Source = 'src-tauri\target\release\epubreader.exe'; Dest = 'epubreader.exe' }
    @{ Source = 'src-tauri\target\release\bundle\nsis\EpubReader_2.0.0_x64-setup.exe'; Dest = 'EpubReader_2.0.0_x64-setup.exe' }
)

function Test-Excluded {
    param($relPath)
    $normalized = $relPath.Replace('/', '\')
    $basename = Split-Path $normalized -Leaf

    foreach ($file in $excludedFiles) {
        if ($basename -eq $file) { return $true }
    }

    foreach ($prefix in $excludedDirPrefixes) {
        if ($normalized -eq $prefix) { return $true }
        if ($normalized.StartsWith($prefix + '\')) { return $true }
        if ($normalized.Contains('\' + $prefix + '\')) { return $true }
    }
    return $false
}

function Get-ZipEntry {
    param($relPath)
    # Map source path to zip path: prebuilt binaries go to root, rest stays in place
    foreach ($b in $rootBinaries) {
        if ($relPath -eq $b.Source) { return $b.Dest }
    }
    return $relPath
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.IO.Compression

Write-Host 'Scanning files...'
$allFiles = @(Get-ChildItem $source -Recurse -Force -ErrorAction SilentlyContinue | Where-Object { -not $_.PSIsContainer })
$totalCount = $allFiles.Count
Write-Host ('Total files found: ' + $totalCount)

$count = 0
$totalSize = [int64]0
$skipped = 0
$rooted = 0

$mode = [System.IO.Compression.ZipArchiveMode]::Create
$archive = [System.IO.Compression.ZipFile]::Open($zipPath, $mode)
try {
    foreach ($file in $allFiles) {
        $relPath = $file.FullName.Substring($source.Length + 1)

        # Pre-built binaries: skip source path, add at zip root
        $isRooted = $false
        foreach ($b in $rootBinaries) {
            if ($relPath -eq $b.Source) {
                $isRooted = $true
                break
            }
        }

        if ($isRooted) {
            $entryName = Get-ZipEntry $relPath
            $rooted++
        } else {
            if (Test-Excluded $relPath) {
                $skipped++
                continue
            }
            $entryName = $relPath
        }

        try {
            [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $archive, $file.FullName, $entryName,
                [System.IO.Compression.CompressionLevel]::Optimal
            ) | Out-Null
            $count++
            $totalSize += $file.Length
        } catch {
            Write-Warning ('Failed: ' + $relPath + ' - ' + $_.Exception.Message)
        }
    }
} finally {
    $archive.Dispose()
}

Write-Host ''
Write-Host '==========================================='
Write-Host ('  Files added:    ' + $count + ' (' + $rooted + ' placed at zip root)')
Write-Host ('  Files skipped:  ' + $skipped)
Write-Host ('  Total size:     ' + [math]::Round($totalSize/1MB, 2) + ' MB')
Write-Host '==========================================='

$zipInfo = Get-Item $zipPath
Write-Host ('  Zip file:       ' + $zipPath)
Write-Host ('  Zip size:       ' + [math]::Round($zipInfo.Length/1MB, 2) + ' MB')
Write-Host ''
Write-Host 'Done.'
