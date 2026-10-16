<#
.SYNOPSIS
  Opens a .docx in Microsoft Word (read-only, hidden) and saves a PDF of it.

.DESCRIPTION
  A developer check for the DOCX exporter, not part of the product or CI. It needs Word on the
  machine. The file is opened without the repair option, so a document Word would have to repair
  fails here with "WORD ERROR". On success it prints one JSON line with the number of pages,
  tables, equations (OMath objects) and pictures Word found. Word is closed afterwards.

.EXAMPLE
  powershell -File scripts/word_render.ps1 -In G:\opendoc-dev\exports\a.docx -Out G:\opendoc-dev\pdf\a.pdf
#>
param(
  [Parameter(Mandatory = $true)][string]$In,
  [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = 'Stop'
$word = $null
try {
  $word = New-Object -ComObject Word.Application
  $word.Visible = $false
  $word.DisplayAlerts = 0
  $missing = [Type]::Missing
  # Documents.Open(FileName, ConfirmConversions, ReadOnly, AddToRecentFiles, ..., OpenAndRepair = $false)
  $doc = $word.Documents.Open($In, $false, $true, $false, $missing, $missing, $false, $missing, $missing, $missing, $missing, $false, $false)
  $info = [ordered]@{
    file         = $In
    compat       = $doc.CompatibilityMode
    paragraphs   = $doc.Paragraphs.Count
    tables       = $doc.Tables.Count
    equations    = $doc.OMaths.Count
    inlineShapes = $doc.InlineShapes.Count
    pages        = $doc.ComputeStatistics(2)
  }
  $path = [ref]$Out
  $pdf = [ref]17  # wdFormatPDF
  $doc.SaveAs2($path, $pdf)
  $discard = [ref]$false
  $doc.Close($discard)
  $info | ConvertTo-Json -Compress
}
catch {
  Write-Output ("WORD ERROR: " + $_.Exception.Message)
  exit 1
}
finally {
  if ($null -ne $word) {
    $discard = [ref]$false
    $word.Quit($discard)
    [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
  }
}
