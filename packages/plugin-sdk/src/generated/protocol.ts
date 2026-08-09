// This file is generated from crates/resolver/src/plugin/protocol.rs.
// Do not edit it directly.

export const PLUGIN_PROTOCOL_SCHEMA_VERSION = 1 as const;

export const PLUGIN_OPERATIONS: readonly PluginOperationV1[] = Object.freeze(["discover","inspect","plan-edits"]);

export type PluginOperationV1 = "discover" | "inspect" | "plan-edits";

export type PluginMetadataV1 = { "schema-version": 1, ecosystem: string, "plugin-version": string, operations: Array<PluginOperationV1>, "read-patterns"?: Array<string>, };

export type PluginDiscoverInputV1 = { "project-root": string, };

export type PluginPackageLocationV1 = { id: string, path: string, };

export type PluginInspectInputV1 = { "project-root": string, package: PluginPackageLocationV1, };

export type PluginDependencyKindV1 = "unspecified" | "runtime" | "development" | "build" | "optional" | "peer";

export type PluginDependencySourceV1 = "manifest" | "config";

export type PluginManifestDependencyV1 = { "manifest-name": string, kind: PluginDependencyKindV1, requirement?: string | null, };

export type PluginVersionSourceV1 = { "kind": "package-manifest" } | { "kind": "shared", manifest: string, field: string, };

export type PluginPackageInspectionV1 = { id: string, "manifest-name": string, version: string, "version-source": PluginVersionSourceV1, ecosystem: string, path: string, publishable: boolean, dependencies: Array<PluginManifestDependencyV1>, };

export type PluginDependencyV1 = { package: string, kind: PluginDependencyKindV1, requirement?: string | null, source: PluginDependencySourceV1, };

export type PluginPackageSnapshotV1 = { id: string, "manifest-name": string, version: string, "version-source": PluginVersionSourceV1, ecosystem: string, path: string, publishable: boolean, dependencies: Array<PluginDependencyV1>, };

export type PluginPlanEditsInputV1 = { "project-root": string, "workspace-packages": Array<PluginPackageSnapshotV1>, "released-packages": Array<string>, versions: { [key in string]: string }, };

export type PluginCallV1 = { "operation": "discover", "input": PluginDiscoverInputV1 } | { "operation": "inspect", "input": PluginInspectInputV1 } | { "operation": "plan-edits", "input": PluginPlanEditsInputV1 };

export type PluginRequestV1 = { "schema-version": 1, } & ({ "operation": "discover", "input": PluginDiscoverInputV1 } | { "operation": "inspect", "input": PluginInspectInputV1 } | { "operation": "plan-edits", "input": PluginPlanEditsInputV1 });

export type PluginDiagnosticSeverityV1 = "info" | "warning" | "error";

export type PluginDiagnosticV1 = { plugin: string, operation: PluginOperationV1, severity: PluginDiagnosticSeverityV1, code: string, message: string, package?: string | null, path?: string | null, };

export type PluginFileEditExpectationV1 = { "kind": "existing", sha256: string, } | { "kind": "missing" };

export type PluginSharedVersionEditV1 = { manifest: string, field: string, packages: Array<string>, };

export type PluginEditSourceV1 = { "kind": "package-version", package: string, } | { "kind": "dependency-version", package: string, dependency: string, } | { "kind": "workspace-dependencies", dependencies: Array<string>, } | { "kind": "workspace-manifest", "shared-versions": Array<PluginSharedVersionEditV1>, dependencies: Array<string>, };

export type PluginFileEditV1 = { path: string, expected: PluginFileEditExpectationV1, "new-content": string, source: PluginEditSourceV1, };

export type PluginOutputV1 = { "operation": "discover", "output": { packages: Array<PluginPackageInspectionV1>, } } | { "operation": "inspect", "output": { package: PluginPackageInspectionV1, } } | { "operation": "plan-edits", "output": { edits: Array<PluginFileEditV1>, } };

export type PluginOutcomeV1 = { "status": "success", output: PluginOutputV1, } | { "status": "failure" };

export type PluginResponseV1 = { "schema-version": 1, diagnostics: Array<PluginDiagnosticV1>, } & ({ "status": "success", output: PluginOutputV1, } | { "status": "failure" });

