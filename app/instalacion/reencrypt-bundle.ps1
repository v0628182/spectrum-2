# re-encrypt profiles.bin from EchoAudioControl key to VanySoundControl key
# AES-256-CBC with PKCS7 padding, format: [16-byte IV][ciphertext]

param(
    [string]$InputFile = (Join-Path $PSScriptRoot "profiles.bin"),
    [string]$OutputFile = (Join-Path $PSScriptRoot "profiles.bin")
)

Add-Type -AssemblyName System.Security

function Get-Sha256([byte[]]$data) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    return $sha.ComputeHash($data)
}

function Decrypt-Bundle([byte[]]$encrypted, [byte[]]$key) {
    if ($encrypted.Length -lt 17) { throw "Bundle too short" }
    $iv = $encrypted[0..15]
    $ciphertext = $encrypted[16..($encrypted.Length - 1)]

    $aes = [System.Security.Cryptography.Aes]::Create()
    $aes.Key = $key
    $aes.IV = $iv
    $aes.Mode = [System.Security.Cryptography.CipherMode]::CBC
    $aes.Padding = [System.Security.Cryptography.PaddingMode]::PKCS7

    $decryptor = $aes.CreateDecryptor()
    $ms = New-Object System.IO.MemoryStream(,$ciphertext)
    $cs = New-Object System.Security.Cryptography.CryptoStream($ms, $decryptor, [System.Security.Cryptography.CryptoStreamMode]::Read)
    $result = New-Object System.IO.MemoryStream
    $cs.CopyTo($result)
    $cs.Dispose()
    $ms.Dispose()
    $aes.Dispose()
    return $result.ToArray()
}

function Encrypt-Bundle([byte[]]$plaintext, [byte[]]$key) {
    $aes = [System.Security.Cryptography.Aes]::Create()
    $aes.Key = $key
    $aes.GenerateIV()
    $aes.Mode = [System.Security.Cryptography.CipherMode]::CBC
    $aes.Padding = [System.Security.Cryptography.PaddingMode]::PKCS7

    $encryptor = $aes.CreateEncryptor()
    $ms = New-Object System.IO.MemoryStream
    $cs = New-Object System.Security.Cryptography.CryptoStream($ms, $encryptor, [System.Security.Cryptography.CryptoStreamMode]::Write)
    $cs.Write($plaintext, 0, $plaintext.Length)
    $cs.FlushFinalBlock()
    $ciphertext = $ms.ToArray()
    $cs.Dispose()
    $ms.Dispose()

    # Output: IV + ciphertext
    $result = New-Object byte[] ($aes.IV.Length + $ciphertext.Length)
    [Array]::Copy($aes.IV, 0, $result, 0, $aes.IV.Length)
    [Array]::Copy($ciphertext, 0, $result, $aes.IV.Length, $ciphertext.Length)
    $aes.Dispose()
    return $result
}

$oldKey = Get-Sha256 ([System.Text.Encoding]::UTF8.GetBytes("EchoAudioControl-Profiles-v1"))
$newKey = Get-Sha256 ([System.Text.Encoding]::UTF8.GetBytes("VanySoundControl-Profiles-v1"))

Write-Host "Old key (hex): $([BitConverter]::ToString($oldKey).Replace('-','').ToLower())"
Write-Host "New key (hex): $([BitConverter]::ToString($newKey).Replace('-','').ToLower())"

$encrypted = [System.IO.File]::ReadAllBytes($InputFile)
Write-Host "Read $($encrypted.Length) bytes from $InputFile"

$plaintext = Decrypt-Bundle $encrypted $oldKey
Write-Host "Decrypted: $($plaintext.Length) bytes"

# Verify EAPF magic
if ($plaintext.Length -ge 4) {
    $magic = [System.Text.Encoding]::ASCII.GetString($plaintext, 0, 4)
    Write-Host "Magic: $magic"
    if ($magic -ne "EAPF") { throw "Invalid bundle magic: $magic" }
}

$reEncrypted = Encrypt-Bundle $plaintext $newKey
Write-Host "Re-encrypted: $($reEncrypted.Length) bytes"

# Verify roundtrip
$verify = Decrypt-Bundle $reEncrypted $newKey
if ([System.Linq.Enumerable]::SequenceEqual([byte[]]$plaintext, [byte[]]$verify)) {
    Write-Host "Roundtrip verification OK"
} else {
    throw "Roundtrip verification FAILED"
}

[System.IO.File]::WriteAllBytes($OutputFile, $reEncrypted)
Write-Host "Written to $OutputFile"
