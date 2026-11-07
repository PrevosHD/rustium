# Memory per app on Windows, as Task Manager's "Memory" column counts it (private working set).
# Sums each app with all its child processes, including the hidden msedgewebview2.exe helpers.
# Usage: paste into PowerShell, or run: powershell -ExecutionPolicy Bypass -File measure-memory.ps1
$mem=@{}; Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | % { $mem[[int]$_.IDProcess]=[long]$_.WorkingSetPrivate }
$all=Get-CimInstance Win32_Process; $byId=@{}; $all | % { $byId[[int]$_.ProcessId]=$_ }
foreach ($app in 'rustium','Ferdium','WhatsApp') {
  $set=@{}; $todo=[System.Collections.Queue]::new()
  $all | ? { $_.Name -like "$app*" -and -not ($byId[[int]$_.ParentProcessId].Name -like "$app*") } | % { $todo.Enqueue($_) }
  while ($todo.Count) { $p=$todo.Dequeue(); if ($set.ContainsKey([int]$p.ProcessId)) { continue }; $set[[int]$p.ProcessId]=$p
    $all | ? { $_.ParentProcessId -eq $p.ProcessId -and $_.CreationDate -ge $p.CreationDate } | % { $todo.Enqueue($_) } }
  $rows=$set.Values | % { [pscustomobject]@{ MB=[math]::Round($mem[[int]$_.ProcessId]/1MB); Name=$_.Name } } | sort MB -desc
  "== $app : $(($rows | measure MB -sum).Sum) MB in $($rows.Count) processes"
  $rows | select -first 5 | % { "   $($_.MB) MB  $($_.Name)" }
}
