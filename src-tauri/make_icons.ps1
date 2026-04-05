New-Item -ItemType Directory -Force -Path icons | Out-Null

# Minimal valid 1x1 32bpp ICO (70 bytes)
# Structure: ICO header (6) + ICONDIRENTRY (16) + BITMAPINFOHEADER (40) + XOR mask (4) + AND mask (4)
$ico_bytes = [byte[]](
    0,0, 1,0, 1,0,                          # ICO header: reserved, type=1, count=1
    1, 1, 0, 0, 1,0, 32,0, 48,0,0,0, 22,0,0,0,  # ICONDIRENTRY: 1x1, 32bpp, size=48, offset=22
    40,0,0,0,                               # BITMAPINFOHEADER size
    1,0,0,0,                                # width = 1
    2,0,0,0,                                # height = 2 (XOR+AND combined)
    1,0,                                    # planes = 1
    32,0,                                   # bitCount = 32
    0,0,0,0, 0,0,0,0,                       # compression, sizeImage
    0,0,0,0, 0,0,0,0,                       # xPels, yPels
    0,0,0,0, 0,0,0,0,                       # clrUsed, clrImportant
    0x5B, 0x6A, 0xF0, 0xFF,                 # XOR mask: BGRA = accent blue opaque
    0,0,0,0                                 # AND mask: fully visible
)
[System.IO.File]::WriteAllBytes("$PSScriptRoot\icons\icon.ico", $ico_bytes)

# Minimal 1x1 PNG (a known-good minimal blue PNG, base64 encoded)
$png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
$png_bytes = [System.Convert]::FromBase64String($png_b64)
[System.IO.File]::WriteAllBytes("$PSScriptRoot\icons\32x32.png",       $png_bytes)
[System.IO.File]::WriteAllBytes("$PSScriptRoot\icons\128x128.png",     $png_bytes)
[System.IO.File]::WriteAllBytes("$PSScriptRoot\icons\128x128@2x.png",  $png_bytes)

# Minimal ICNS stub (macOS, 8-byte header only)
$icns_bytes = [byte[]](0x69,0x63,0x6E,0x73, 0,0,0,8)
[System.IO.File]::WriteAllBytes("$PSScriptRoot\icons\icon.icns", $icns_bytes)

Write-Host "Icons created:" (Get-ChildItem "$PSScriptRoot\icons").Name
