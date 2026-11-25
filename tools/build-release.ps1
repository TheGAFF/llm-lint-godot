#Requires -Version 7.4
param(
    [Parameter(Mandatory)]
    [string]$Target,
    [string]$GdstyleArchive
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if ($GdstyleArchive) {
    $GdstyleArchive = (Resolve-Path -LiteralPath $GdstyleArchive).Path
}

Push-Location (Join-Path $PSScriptRoot '..')
try {
    $repository = (Get-Location).Path
    $manifest = Get-Content -Raw policy/gdstyle-release.json | ConvertFrom-Json
    $gdstyle = $manifest | Where-Object target -CEQ $Target
    $hostLine = rustc -vV | Where-Object { $_.StartsWith('host: ') }
    if (!$gdstyle -or $hostLine -cne "host: $Target") {
        throw "Expected a native host with a pinned gdstyle release for $Target. Found $hostLine."
    }
    $metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    $version = ($metadata.packages | Where-Object name -CEQ 'godot-llm-lint').version
    $executable = if ($IsWindows) { 'godot-llm-lint.exe' } else { 'godot-llm-lint' }
    $extension = if ($IsWindows) { 'zip' } else { 'tar.gz' }
    $archiveName = "godot-llm-lint-v$version-$Target.$extension"

    cargo build --release --locked --target $Target
    $built = Join-Path $metadata.target_directory "$Target/release/$executable"

    $output = Join-Path $repository ".verification/release/$Target"
    $work = Join-Path $output ([guid]::NewGuid().ToString('N'))
    $staging = Join-Path $work 'staging'
    $unpacked = Join-Path $work 'unpacked'
    $project = Join-Path $work 'project'
    $dist = Join-Path $output 'dist'
    New-Item -ItemType Directory -Path $staging, $unpacked, $dist -Force | Out-Null
    Copy-Item -LiteralPath 'examples/quickstart' -Destination $project -Recurse

    Copy-Item -LiteralPath $built, 'LICENSE', 'THIRD_PARTY_NOTICES.txt' -Destination $staging
    $rustNotices = Join-Path (rustc --print sysroot) 'share/doc/rust/COPYRIGHT-library.html'
    $rustNoticesName = 'RUST_STANDARD_LIBRARY_NOTICES.html'
    Copy-Item -LiteralPath $rustNotices -Destination (Join-Path $staging $rustNoticesName)
    $fileNames = @($executable, 'LICENSE', 'THIRD_PARTY_NOTICES.txt', $rustNoticesName)
    $archive = Join-Path $work $archiveName
    if ($IsWindows) {
        $files = $fileNames | ForEach-Object { Join-Path $staging $_ }
        Compress-Archive -LiteralPath $files -DestinationPath $archive
    } else {
        chmod +x (Join-Path $staging $executable)
        tar -czf $archive -C $staging @fileNames
    }
    $checksum = "$archive.sha256"
    $digest = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($checksum, "$digest  $archiveName`n")
    $expected = (Get-Content -LiteralPath $checksum -Raw).Trim() -split '  ', 2
    if ($expected[1] -cne $archiveName -or
        (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ine $expected[0]) {
        throw "Archive checksum verification failed for $archiveName."
    }
    if ($IsWindows) {
        Expand-Archive -LiteralPath $archive -DestinationPath $unpacked
    } else {
        tar -xzf $archive -C $unpacked
    }
    foreach ($fileName in $fileNames) {
        if (!(Test-Path -LiteralPath (Join-Path $unpacked $fileName) -PathType Leaf)) {
            throw "$archiveName is missing $fileName."
        }
    }

    $binary = Join-Path $unpacked $executable
    if ((& $binary --version) -cne "godot-llm-lint $version") {
        throw "Unexpected binary version in $archiveName."
    }
    $initArguments = @('init')
    if ($GdstyleArchive) {
        $initArguments += @('--gdstyle-archive', $GdstyleArchive)
    }
    $previousCache = $env:GODOT_LLM_LINT_CACHE_DIR
    $previousGdstyle = $env:GDSTYLE_TEST_BINARY
    try {
        $env:GODOT_LLM_LINT_CACHE_DIR = Join-Path $work 'tool-cache'
        Push-Location $project
        try {
            & $binary @initArguments
            & $binary sync --check
            & $binary check
            & $binary check --json | ConvertFrom-Json | Out-Null
        } finally {
            Pop-Location
        }
        $nativeBinary = Join-Path $env:GODOT_LLM_LINT_CACHE_DIR "gdstyle/0.8.0/$Target/$($gdstyle.executable)"
        if ((Get-FileHash -LiteralPath $nativeBinary -Algorithm SHA256).Hash -ine $gdstyle.executable_sha256) {
            throw 'gdstyle executable differs from the pinned release.'
        }
        $env:GDSTYLE_TEST_BINARY = $nativeBinary
        cargo test --locked
    } finally {
        $env:GODOT_LLM_LINT_CACHE_DIR = $previousCache
        $env:GDSTYLE_TEST_BINARY = $previousGdstyle
    }

    Copy-Item -LiteralPath $archive, $checksum -Destination $dist -Force
    Write-Output "Verified $archiveName and its checksum in $dist."
} finally {
    Pop-Location
}
