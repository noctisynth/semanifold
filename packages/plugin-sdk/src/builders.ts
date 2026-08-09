import type { PluginHostV1 } from './capabilities.js';
import {
  type EcosystemIdV1,
  PLUGIN_OPERATIONS,
  PLUGIN_PROTOCOL_SCHEMA_VERSION,
  type PluginDiagnosticDetailsV1,
  type PluginDiagnosticInputV1,
  type PluginDiagnosticSeverityV1,
  type PluginDiagnosticV1,
  type PluginFailureResponseV1,
  type PluginMetadataV1,
  type PluginOperationV1,
  type PluginOutputByOperationV1,
  type PluginRequestForOperationV1,
  type PluginRequestV1,
  type PluginResponseV1,
  type PluginSuccessResponseV1,
  type SemVerV1,
} from './protocol.js';

export interface DefinePluginMetadataOptionsV1 {
  readonly ecosystem: EcosystemIdV1;
  readonly pluginVersion: SemVerV1;
  readonly readPatterns?: readonly string[];
}

export function definePluginMetadata(
  options: DefinePluginMetadataOptionsV1,
): PluginMetadataV1 {
  const operations = Object.freeze([...PLUGIN_OPERATIONS]);
  const readPatterns = Object.freeze(
    [...new Set(options.readPatterns ?? [])].sort(),
  );
  const metadata: PluginMetadataV1 =
    readPatterns.length === 0
      ? {
          'schema-version': PLUGIN_PROTOCOL_SCHEMA_VERSION,
          ecosystem: options.ecosystem,
          'plugin-version': options.pluginVersion,
          operations,
        }
      : {
          'schema-version': PLUGIN_PROTOCOL_SCHEMA_VERSION,
          ecosystem: options.ecosystem,
          'plugin-version': options.pluginVersion,
          operations,
          'read-patterns': readPatterns,
        };

  return Object.freeze(metadata);
}

export type PluginEntrypointV1 = (
  request: PluginRequestV1,
  host: PluginHostV1,
) => PluginResponseV1 | PromiseLike<PluginResponseV1>;

export function definePlugin(
  entrypoint: PluginEntrypointV1,
): PluginEntrypointV1 {
  return entrypoint;
}

export function createPluginDiagnostic<
  Severity extends PluginDiagnosticSeverityV1,
>(
  request: PluginRequestV1,
  plugin: EcosystemIdV1,
  input: PluginDiagnosticInputV1<Severity>,
): PluginDiagnosticV1<Severity> {
  return {
    plugin,
    operation: request.operation,
    ...input,
  };
}

export function createPluginSuccess<Operation extends PluginOperationV1>(
  request: PluginRequestForOperationV1<Operation>,
  output: PluginOutputByOperationV1[Operation],
  diagnostics: readonly PluginDiagnosticV1[] = [],
): PluginSuccessResponseV1<Operation> {
  return {
    'schema-version': PLUGIN_PROTOCOL_SCHEMA_VERSION,
    diagnostics: [...diagnostics],
    status: 'success',
    output: {
      operation: request.operation,
      output,
    },
  };
}

export function createPluginFailure(
  request: PluginRequestV1,
  plugin: EcosystemIdV1,
  error: PluginDiagnosticDetailsV1,
  diagnostics: readonly PluginDiagnosticV1[] = [],
): PluginFailureResponseV1 {
  return {
    'schema-version': PLUGIN_PROTOCOL_SCHEMA_VERSION,
    diagnostics: [
      createPluginDiagnostic(request, plugin, {
        severity: 'error',
        ...error,
      }),
      ...diagnostics,
    ],
    status: 'failure',
  };
}
