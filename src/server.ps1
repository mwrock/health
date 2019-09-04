function Restore-Environment($origEnv) {
    $origEnv.keys | % {
        New-Item -ItemType Variable -Path Env: -Name $_ -Value $origEnv[$_] -Force | Out-Null
    }
    Get-ChildItem env:\ | ? { !$origEnv.ContainsKey($_.name) } | % {
        Remove-Item -Path (Join-Path Env: $_.Name) | Out-Null
    }
}

Write-Host "Starting PS pipe server with PID $PID"
try {
    $np = new-object System.IO.Pipes.NamedPipeServerStream('rust-ipc-bdd62f4b-2d3f-409c-a82d-5530be2ae8a1', [System.IO.Pipes.PipeDirection]::InOut)
    Write-Host "Named pipe created. Waiting for connection..."
    $running = $true
 
    $origEnv = @{}
    Get-ChildItem env:\ | % {
        $origEnv[$_.name] = $_.Value
    }

    while($running) {
        $np.WaitForConnection()
        write-host "pipe is connected"
        $byte = $np.ReadByte()
        write-host "i have read a byte"
        if($byte -eq 1) {
            Write-host "checking health..."
            & "c:/dev/health/src/hook.ps1"
            $np.Write([System.BitConverter]::GetBytes($LASTEXITCODE), 0, 4)
            $np.Flush()
            Write-Host "Before restore: ${env:blah}"
            Restore-Environment $origEnv
            Write-Host "After restore: ${env:blah}"
        }
        else {
            Write-Host "no file given. quitting."
            $running = $false
        }
        $np.Disconnect()
    }
} finally {
    $np.Dispose()
    Write-Host "exiting $PID"
}
