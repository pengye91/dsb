{{/*
DSB Helm Chart Helper Templates

This file contains common helper templates used across the DSB Helm chart.
*/}}

{{/*
Expand the name of the chart.
*/}}
{{- define "dsb.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "dsb.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "dsb.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "dsb.labels" -}}
helm.sh/chart: {{ include "dsb.chart" . }}
{{ include "dsb.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "dsb.selectorLabels" -}}
app.kubernetes.io/name: {{ include "dsb.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "dsb.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "dsb.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create the name of the secret to use for API keys
*/}}
{{- define "dsb.apiSecretName" -}}
{{- if .Values.secrets.existingSecret }}
{{- .Values.secrets.existingSecret }}
{{- else }}
{{- printf "%s-api-keys" (include "dsb.fullname" .) }}
{{- end }}
{{- end }}

{{/*
In-cluster URL for the DSB HTTP API (used by SSH gateway cleanup, etc.).
Docker Compose uses hostname dsb-server; in Helm the Service is the chart fullname (e.g. dsb).
*/}}
{{- define "dsb.sshApiUrl" -}}
{{- if .Values.config.ssh.apiUrl }}
{{- .Values.config.ssh.apiUrl }}
{{- else }}
{{- printf "http://%s.%s.svc.cluster.local:%v" (include "dsb.fullname" .) .Release.Namespace .Values.config.server.port }}
{{- end }}
{{- end }}

{{/*
Create the name of the secret to use for PostgreSQL password
*/}}
{{- define "dsb.postgresSecretName" -}}
{{- if .Values.postgres.existingSecret }}
{{- .Values.postgres.existingSecret }}
{{- else }}
{{- printf "%s-postgres" (include "dsb.fullname" .) }}
{{- end }}
{{- end }}

{{/*
Generate a random API key
*/}}
{{- define "dsb.generateApiKey" -}}
{{- randAlphaNum 64 }}
{{- end }}

{{/*
Safely get a nested searxng value with a default.
Usage: {{ include "dsb.searxngValue" (dict "root" . "key" "replicaCount" "default" 1) }}
*/}}
{{- define "dsb.searxngValue" -}}
{{- $root := .root -}}
{{- $key := .key -}}
{{- $default := .default -}}
{{- $searxng := default dict $root.Values.searxng -}}
{{- if eq $key "repository" -}}
{{- $image := default dict $searxng.image -}}
{{- default $default $image.repository -}}
{{- else if eq $key "tag" -}}
{{- $image := default dict $searxng.image -}}
{{- default $default $image.tag -}}
{{- else if eq $key "pullPolicy" -}}
{{- $image := default dict $searxng.image -}}
{{- default $default $image.pullPolicy -}}
{{- else if eq $key "replicaCount" -}}
{{- default $default $searxng.replicaCount -}}
{{- else if eq $key "secretKey" -}}
{{- default $default $searxng.secretKey -}}
{{- else if eq $key "resources" -}}
{{- default $default $searxng.resources -}}
{{- else -}}
{{- $default -}}
{{- end -}}
{{- end }}
