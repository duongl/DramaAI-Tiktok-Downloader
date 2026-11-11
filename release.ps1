param(
    [string]$Version,
    [ValidateSet("patch", "minor", "major")]
    [string]$Bump,
    [switch]$SkipBuild,
    [switch]$NoPush,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
Set-Location $root

$versionPath = Join-Path $root "VERSION"
$cargoPath = Join-Path $root "gui\Cargo.toml"
$setupPath = Join-Path $root "setup.py"
$mainPath = Join-Path $root "gui\src\main.rs"

foreach ($path in @($cargoPath, $setupPath, $mainPath)) {
    if (-not (Test-Path $path)) {
        throw "Missing file: $path"
    }
}

$current = if (Test-Path $versionPath) {
    (Get-Content $versionPath -Raw).Trim()
} else {
    $match = [regex]::Match(
        (Get-Content $cargoPath -Raw),
        '(?m)^version\s*=\s*"([^"]+)"'
    )
    if (-not $match.Success) {
        throw "Cannot determine current version"
    }
    $match.Groups[1].Value
}

if ($current -notmatch '^\d+\.\d+\.\d+$') {
    throw "Current version is not semver: $current"
}
if ($Version -and $Bump) {
    throw "Use either -Version or -Bump, not both"
}

if ($Version) {
    $new = $Version.Trim().TrimStart("v")
} elseif ($Bump) {
    $parts = $current.Split(".") | ForEach-Object { [int]$_ }
    switch ($Bump) {
        "major" { $parts[0]++; $parts[1] = 0; $parts[2] = 0 }
        "minor" { $parts[1]++; $parts[2] = 0 }
        "patch" { $parts[2]++ }
    }
    $new = "{0}.{1}.{2}" -f $parts[0], $parts[1], $parts[2]
} else {
    throw "Specify -Version x.y.z or -Bump patch|minor|major"
}

if ($new -notmatch '^\d+\.\d+\.\d+$') {
    throw "New version is not semver: $new"
}
if ([version]$new -le [version]$current) {
    throw "New version $new must be greater than current version $current"
}
$existingTag = git tag --list "v$new"
if ($existingTag) {
    throw "Tag v$new already exists"
}

$oldTag = "v$current"
$newTag = "v$new"

Write-Host "Current version: $current"
Write-Host "New version:     $new"

if ($DryRun) {
    Write-Host ""
    Write-Host "[dry-run] Would update:"
    Write-Host "  VERSION"
    Write-Host "  gui/Cargo.toml"
    Write-Host "  setup.py"
    Write-Host "  gui/src/main.rs ($oldTag -> $newTag)"
    Write-Host "  gui/Cargo.lock (via cargo build)"
    Write-Host "[dry-run] No files changed."
    return
}

$status = git status --porcelain
if ($LASTEXITCODE -ne 0) {
    throw "Not a git repository"
}
if ($status) {
    Write-Host "Note: existing uncommitted changes will be included in the release commit." -ForegroundColor Yellow
}

[IO.File]::WriteAllText(
    $versionPath,
    "$new`n",
    (New-Object System.Text.UTF8Encoding($false))
)

$cargo = [IO.File]::ReadAllText($cargoPath)
$cargoVersion = [regex]'(?m)^version\s*=\s*"[^"]+"'
if (-not $cargoVersion.IsMatch($cargo)) {
    throw "Cannot find package version in gui/Cargo.toml"
}
$cargo = $cargoVersion.Replace($cargo, "version = `"$new`"", 1)
[IO.File]::WriteAllText($cargoPath, $cargo, (New-Object System.Text.UTF8Encoding($false)))

$setup = [IO.File]::ReadAllText($setupPath)
$setupVersion = [regex]'version\s*=\s*"[^"]+"'
if (-not $setupVersion.IsMatch($setup)) {
    throw "Cannot find version in setup.py"
}
$setup = $setupVersion.Replace($setup, "version=`"$new`"", 1)
[IO.File]::WriteAllText($setupPath, $setup, (New-Object System.Text.UTF8Encoding($false)))

$main = [IO.File]::ReadAllText($mainPath)
$tagMatches = ([regex]::Matches($main, [regex]::Escape($oldTag))).Count
if ($tagMatches -eq 0) {
    throw "Cannot find $oldTag in gui/src/main.rs"
}
$main = $main.Replace($oldTag, $newTag)
[IO.File]::WriteAllText($mainPath, $main, (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Updated VERSION, Cargo.toml, setup.py and GUI title." -ForegroundColor Green

if (-not $SkipBuild) {
    Write-Host "Building release (this also updates Cargo.lock)..." -ForegroundColor Cyan
    & (Join-Path $root "gui\build.ps1")
    if (-not $?) {
        throw "Build failed"
    }

    Write-Host "Running formatting and tests..." -ForegroundColor Cyan
    & cargo fmt --manifest-path $cargoPath
    if ($LASTEXITCODE -ne 0) {
        throw "cargo fmt failed"
    }
    & cargo test --manifest-path $cargoPath
    if ($LASTEXITCODE -ne 0) {
        throw "cargo test failed"
    }

    Write-Host "Running Python tests..." -ForegroundColor Cyan
    & python -X utf8 -m unittest discover -s tests
    if ($LASTEXITCODE -ne 0) {
        throw "Python tests failed"
    }
} else {
    Write-Host "Skipping build/test (-SkipBuild)." -ForegroundColor Yellow
}

git add -A
if ($LASTEXITCODE -ne 0) {
    throw "git add failed"
}
git commit -m "Release v$new"
if ($LASTEXITCODE -ne 0) {
    throw "git commit failed"
}
git tag -a "v$new" -m "Release v$new"
if ($LASTEXITCODE -ne 0) {
    throw "git tag failed"
}

if (-not $NoPush) {
    git push origin main
    if ($LASTEXITCODE -ne 0) {
        throw "git push main failed"
    }
    git push origin "v$new"
    if ($LASTEXITCODE -ne 0) {
        throw "git push tag failed"
    }
    Write-Host "Pushed v$new. GitHub Actions will publish the release." -ForegroundColor Green
} else {
    Write-Host "Created local commit/tag v$new (-NoPush)." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Done: v$new" -ForegroundColor Green
