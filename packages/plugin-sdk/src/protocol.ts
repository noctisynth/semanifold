import type {
  PluginDependencyKindV1 as GeneratedPluginDependencyKindV1,
  PluginDependencySourceV1 as GeneratedPluginDependencySourceV1,
  PluginDependencyV1 as GeneratedPluginDependencyV1,
  PluginDiagnosticSeverityV1 as GeneratedPluginDiagnosticSeverityV1,
  PluginDiagnosticV1 as GeneratedPluginDiagnosticV1,
  PluginDiscoverInputV1 as GeneratedPluginDiscoverInputV1,
  PluginEditSourceV1 as GeneratedPluginEditSourceV1,
  PluginFileEditExpectationV1 as GeneratedPluginFileEditExpectationV1,
  PluginFileEditV1 as GeneratedPluginFileEditV1,
  PluginInspectInputV1 as GeneratedPluginInspectInputV1,
  PluginManifestDependencyV1 as GeneratedPluginManifestDependencyV1,
  PluginMetadataV1 as GeneratedPluginMetadataV1,
  PluginOperationV1 as GeneratedPluginOperationV1,
  PluginOutputV1 as GeneratedPluginOutputV1,
  PluginPackageInspectionV1 as GeneratedPluginPackageInspectionV1,
  PluginPackageLocationV1 as GeneratedPluginPackageLocationV1,
  PluginPackageSnapshotV1 as GeneratedPluginPackageSnapshotV1,
  PluginPlanEditsInputV1 as GeneratedPluginPlanEditsInputV1,
  PluginRequestV1 as GeneratedPluginRequestV1,
  PluginResponseV1 as GeneratedPluginResponseV1,
  PluginSharedVersionEditV1 as GeneratedPluginSharedVersionEditV1,
  PluginVersionSourceV1 as GeneratedPluginVersionSourceV1,
} from './generated/protocol.js';

export {
  PLUGIN_OPERATIONS,
  PLUGIN_PROTOCOL_SCHEMA_VERSION,
} from './generated/protocol.js';

type DeepReadonly<Value> = Value extends readonly unknown[]
  ? { readonly [Key in keyof Value]: DeepReadonly<Value[Key]> }
  : Value extends object
    ? { readonly [Key in keyof Value]: DeepReadonly<Value[Key]> }
    : Value;

export type EcosystemIdV1 = string;
export type PackageIdV1 = string;
export type SemVerV1 = string;

export type PluginOperationV1 = GeneratedPluginOperationV1;
export type PluginMetadataV1 = DeepReadonly<GeneratedPluginMetadataV1>;
export type PluginDiscoverInputV1 = DeepReadonly<GeneratedPluginDiscoverInputV1>;
export type PluginInspectInputV1 = DeepReadonly<GeneratedPluginInspectInputV1>;
export type PluginPlanEditsInputV1 = DeepReadonly<GeneratedPluginPlanEditsInputV1>;
export type PluginPackageLocationV1 = DeepReadonly<GeneratedPluginPackageLocationV1>;
export type PluginVersionSourceV1 = DeepReadonly<GeneratedPluginVersionSourceV1>;
export type PluginDependencyKindV1 = GeneratedPluginDependencyKindV1;
export type PluginDependencySourceV1 = GeneratedPluginDependencySourceV1;
export type PluginManifestDependencyV1 = DeepReadonly<GeneratedPluginManifestDependencyV1>;
export type PluginDependencyV1 = DeepReadonly<GeneratedPluginDependencyV1>;
export type PluginPackageInspectionV1 = DeepReadonly<GeneratedPluginPackageInspectionV1>;
export type PluginPackageSnapshotV1 = DeepReadonly<GeneratedPluginPackageSnapshotV1>;
export type PluginFileEditExpectationV1 = DeepReadonly<GeneratedPluginFileEditExpectationV1>;
export type PluginSharedVersionEditV1 = DeepReadonly<GeneratedPluginSharedVersionEditV1>;
export type PluginEditSourceV1 = DeepReadonly<GeneratedPluginEditSourceV1>;
export type PluginFileEditV1 = DeepReadonly<GeneratedPluginFileEditV1>;
export type PluginDiagnosticSeverityV1 = GeneratedPluginDiagnosticSeverityV1;

type GeneratedDiagnosticV1 = DeepReadonly<GeneratedPluginDiagnosticV1>;

export type PluginDiagnosticDetailsV1 = Pick<
  GeneratedDiagnosticV1,
  'code' | 'message' | 'package' | 'path'
>;

export type PluginDiagnosticInputV1<
  Severity extends PluginDiagnosticSeverityV1 = PluginDiagnosticSeverityV1,
> = PluginDiagnosticDetailsV1 & {
  readonly severity: Severity;
};

export type PluginDiagnosticV1<
  Severity extends PluginDiagnosticSeverityV1 = PluginDiagnosticSeverityV1,
> = Omit<GeneratedDiagnosticV1, 'severity'> & {
  readonly severity: Severity;
};

export type PluginRequestV1 = DeepReadonly<GeneratedPluginRequestV1>;

export type PluginRequestForOperationV1<Operation extends PluginOperationV1> =
  Extract<PluginRequestV1, { readonly operation: Operation }>;

export type PluginDiscoverRequestV1 =
  PluginRequestForOperationV1<'discover'>;
export type PluginInspectRequestV1 = PluginRequestForOperationV1<'inspect'>;
export type PluginPlanEditsRequestV1 =
  PluginRequestForOperationV1<'plan-edits'>;

type PluginOutputUnionV1 = DeepReadonly<GeneratedPluginOutputV1>;

type PluginOutputEnvelopeByOperationV1 = {
  readonly [Operation in PluginOperationV1]: Extract<
    PluginOutputUnionV1,
    { readonly operation: Operation }
  >;
};

export type PluginOutputByOperationV1 = {
  readonly [Operation in PluginOperationV1]: PluginOutputEnvelopeByOperationV1[Operation]['output'];
};

export interface PluginOutputEnvelopeV1<
  Operation extends PluginOperationV1 = PluginOperationV1,
> {
  readonly operation: Operation;
  readonly output: PluginOutputByOperationV1[Operation];
}

export type PluginDiscoverOutputV1 = PluginOutputByOperationV1['discover'];
export type PluginInspectOutputV1 = PluginOutputByOperationV1['inspect'];
export type PluginPlanEditsOutputV1 =
  PluginOutputByOperationV1['plan-edits'];

export type PluginResponseV1 = DeepReadonly<GeneratedPluginResponseV1>;

type PluginSuccessResponseUnionV1 = Extract<
  PluginResponseV1,
  { readonly status: 'success' }
>;

export type PluginSuccessResponseV1<
  Operation extends PluginOperationV1 = PluginOperationV1,
> = Omit<PluginSuccessResponseUnionV1, 'output'> & {
  readonly output: PluginOutputEnvelopeV1<Operation>;
};

export type PluginFailureResponseV1 = Extract<
  PluginResponseV1,
  { readonly status: 'failure' }
>;
