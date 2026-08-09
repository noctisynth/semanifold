export const PLUGIN_PROTOCOL_SCHEMA_VERSION = 1 as const;

export type EcosystemIdV1 = string;
export type PackageIdV1 = string;
export type SemVerV1 = string;

export type PluginOperationV1 = 'discover' | 'inspect' | 'plan-edits';

export const PLUGIN_OPERATIONS: readonly PluginOperationV1[] = Object.freeze([
  'discover',
  'inspect',
  'plan-edits',
]);

export interface PluginMetadataV1 {
  readonly 'schema-version': typeof PLUGIN_PROTOCOL_SCHEMA_VERSION;
  readonly ecosystem: EcosystemIdV1;
  readonly 'plugin-version': SemVerV1;
  readonly operations: readonly PluginOperationV1[];
  readonly 'read-patterns'?: readonly string[];
}

export interface PluginDiscoverInputV1 {
  readonly 'project-root': string;
}

export interface PluginInspectInputV1 {
  readonly 'project-root': string;
  readonly package: PluginPackageLocationV1;
}

export interface PluginPlanEditsInputV1 {
  readonly 'project-root': string;
  readonly 'workspace-packages': readonly PluginPackageSnapshotV1[];
  readonly 'released-packages': readonly PackageIdV1[];
  readonly versions: Readonly<Record<PackageIdV1, SemVerV1>>;
}

export interface PluginDiscoverRequestV1 {
  readonly 'schema-version': typeof PLUGIN_PROTOCOL_SCHEMA_VERSION;
  readonly operation: 'discover';
  readonly input: PluginDiscoverInputV1;
}

export interface PluginInspectRequestV1 {
  readonly 'schema-version': typeof PLUGIN_PROTOCOL_SCHEMA_VERSION;
  readonly operation: 'inspect';
  readonly input: PluginInspectInputV1;
}

export interface PluginPlanEditsRequestV1 {
  readonly 'schema-version': typeof PLUGIN_PROTOCOL_SCHEMA_VERSION;
  readonly operation: 'plan-edits';
  readonly input: PluginPlanEditsInputV1;
}

export type PluginRequestV1 =
  | PluginDiscoverRequestV1
  | PluginInspectRequestV1
  | PluginPlanEditsRequestV1;

export type PluginRequestForOperationV1<Operation extends PluginOperationV1> =
  Extract<PluginRequestV1, { readonly operation: Operation }>;

export interface PluginPackageLocationV1 {
  readonly id: PackageIdV1;
  readonly path: string;
}

export type PluginVersionSourceV1 =
  | { readonly kind: 'package-manifest' }
  | {
      readonly kind: 'shared';
      readonly manifest: string;
      readonly field: string;
    };

export type PluginDependencyKindV1 =
  | 'unspecified'
  | 'runtime'
  | 'development'
  | 'build'
  | 'optional'
  | 'peer';

export type PluginDependencySourceV1 = 'manifest' | 'config';

export interface PluginManifestDependencyV1 {
  readonly 'manifest-name': string;
  readonly kind: PluginDependencyKindV1;
  readonly requirement?: string;
}

export interface PluginDependencyV1 {
  readonly package: PackageIdV1;
  readonly kind: PluginDependencyKindV1;
  readonly requirement?: string;
  readonly source: PluginDependencySourceV1;
}

export interface PluginPackageInspectionV1 {
  readonly id: PackageIdV1;
  readonly 'manifest-name': string;
  readonly version: SemVerV1;
  readonly 'version-source': PluginVersionSourceV1;
  readonly ecosystem: EcosystemIdV1;
  readonly path: string;
  readonly publishable: boolean;
  readonly dependencies: readonly PluginManifestDependencyV1[];
}

export interface PluginPackageSnapshotV1 {
  readonly id: PackageIdV1;
  readonly 'manifest-name': string;
  readonly version: SemVerV1;
  readonly 'version-source': PluginVersionSourceV1;
  readonly ecosystem: EcosystemIdV1;
  readonly path: string;
  readonly publishable: boolean;
  readonly dependencies: readonly PluginDependencyV1[];
}

export type PluginFileEditExpectationV1 =
  | { readonly kind: 'existing'; readonly sha256: string }
  | { readonly kind: 'missing' };

export interface PluginSharedVersionEditV1 {
  readonly manifest: string;
  readonly field: string;
  readonly packages: readonly PackageIdV1[];
}

export type PluginEditSourceV1 =
  | { readonly kind: 'package-version'; readonly package: PackageIdV1 }
  | {
      readonly kind: 'dependency-version';
      readonly package: PackageIdV1;
      readonly dependency: PackageIdV1;
    }
  | {
      readonly kind: 'workspace-dependencies';
      readonly dependencies: readonly PackageIdV1[];
    }
  | {
      readonly kind: 'workspace-manifest';
      readonly 'shared-versions': readonly PluginSharedVersionEditV1[];
      readonly dependencies: readonly PackageIdV1[];
    };

export interface PluginFileEditV1 {
  readonly path: string;
  readonly expected: PluginFileEditExpectationV1;
  readonly 'new-content': string;
  readonly source: PluginEditSourceV1;
}

export type PluginDiagnosticSeverityV1 = 'info' | 'warning' | 'error';

export interface PluginDiagnosticDetailsV1 {
  readonly code: string;
  readonly message: string;
  readonly package?: PackageIdV1;
  readonly path?: string;
}

export interface PluginDiagnosticInputV1<
  Severity extends PluginDiagnosticSeverityV1 = PluginDiagnosticSeverityV1,
> extends PluginDiagnosticDetailsV1 {
  readonly severity: Severity;
}

export interface PluginDiagnosticV1<
  Severity extends PluginDiagnosticSeverityV1 = PluginDiagnosticSeverityV1,
> extends PluginDiagnosticInputV1<Severity> {
  readonly plugin: EcosystemIdV1;
  readonly operation: PluginOperationV1;
}

export interface PluginDiscoverOutputV1 {
  readonly packages: readonly PluginPackageInspectionV1[];
}

export interface PluginInspectOutputV1 {
  readonly package: PluginPackageInspectionV1;
}

export interface PluginPlanEditsOutputV1 {
  readonly edits: readonly PluginFileEditV1[];
}

export interface PluginOutputByOperationV1 {
  readonly discover: PluginDiscoverOutputV1;
  readonly inspect: PluginInspectOutputV1;
  readonly 'plan-edits': PluginPlanEditsOutputV1;
}

export interface PluginOutputEnvelopeV1<
  Operation extends PluginOperationV1 = PluginOperationV1,
> {
  readonly operation: Operation;
  readonly output: PluginOutputByOperationV1[Operation];
}

export interface PluginSuccessResponseV1<
  Operation extends PluginOperationV1 = PluginOperationV1,
> {
  readonly 'schema-version': typeof PLUGIN_PROTOCOL_SCHEMA_VERSION;
  readonly diagnostics: readonly PluginDiagnosticV1[];
  readonly status: 'success';
  readonly output: PluginOutputEnvelopeV1<Operation>;
}

export interface PluginFailureResponseV1 {
  readonly 'schema-version': typeof PLUGIN_PROTOCOL_SCHEMA_VERSION;
  readonly diagnostics: readonly PluginDiagnosticV1[];
  readonly status: 'failure';
}

type PluginSuccessResponseUnionV1 = {
  readonly [Operation in PluginOperationV1]: PluginSuccessResponseV1<Operation>;
}[PluginOperationV1];

export type PluginResponseV1 =
  | PluginSuccessResponseUnionV1
  | PluginFailureResponseV1;
