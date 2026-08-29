param(
    [Parameter(Mandatory = $true)]
    [string]$DocumentPath,
    [string]$SchemaPath
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($SchemaPath)) {
    $SchemaPath = Join-Path $PSScriptRoot '..\external tools\cpacs_schema.xsd'
}
$document = [System.IO.Path]::GetFullPath($DocumentPath)
$schema = [System.IO.Path]::GetFullPath($SchemaPath)

if (-not [System.IO.File]::Exists($document)) {
    throw "CPACS document does not exist: $document"
}
if (-not [System.IO.File]::Exists($schema)) {
    throw "CPACS 3.5 schema does not exist: $schema"
}

$schemas = [System.Xml.Schema.XmlSchemaSet]::new()
$schemas.Add('', $schema) | Out-Null
$settings = [System.Xml.XmlReaderSettings]::new()
$settings.ValidationType = [System.Xml.ValidationType]::Schema
$settings.Schemas = $schemas
$messages = [System.Collections.Generic.List[string]]::new()
$settings.add_ValidationEventHandler(
    [System.Xml.Schema.ValidationEventHandler] {
        param($sender, $event)
        $messages.Add("$($event.Severity): $($event.Message)")
    }
)

$reader = [System.Xml.XmlReader]::Create($document, $settings)
try {
    while ($reader.Read()) { }
}
finally {
    $reader.Close()
}

if ($messages.Count -gt 0) {
    $messages
    exit 1
}

Write-Output "CPACS 3.5 validation passed: $document"
