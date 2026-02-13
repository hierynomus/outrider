{{- define "outrider.labels" -}}
app.kubernetes.io/name: {{ include "outrider.fullname" . }}
app: {{ include "outrider.fullname" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | default .Chart.Version }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "outrider.fullname" }}
{{- if .Values.fullnameOverride -}}
{{ .Values.fullnameOverride | trim -}}
{{- else -}}
{{- if .Values.nameOverride -}}
{{ .Values.nameOverride | trim -}}
{{- else -}}
{{ .Release.Name | trim -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "outrider.serviceAccountName" -}}
{{- if .Values.serviceAccount.enabled -}}
{{- include "outrider.fullname" . }}
{{- else -}}
{{ .Values.serviceAccount.name | trim }}
{{- end -}}
{{- end -}}
