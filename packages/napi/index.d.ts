// TypeScript types for @oxc-react-docgen/napi
// Matches the ExtractionOutput shape from crates/core/src/types.rs

export interface ExtractOptions {
  srcDirs: string[]
  exclude?: string[]
  reactVersion?: 'react18' | 'react19'
  crossPackage?: boolean
  pandacssOutdir?: string
  variantFunctions?: string[]
  skipHtmlProps?: boolean
  tsconfigPath?: string
  extraBuiltins?: string[]
  vanillaExtract?: boolean
  cacheDir?: string
  resolveComplexTypes?: boolean
  /** JSON: Record<string, string[]> — additional path alias mappings */
  extraPathsJson?: string
  /** JSON: Record<string, {kind: ...}> — override resolution for specific type names */
  knownTypeOverridesJson?: string
}

export interface InheritedLayer {
  typeName: string
  fileName: string
  omitted: string[]
  htmlElement: string | null
  totalProps: number
}

export interface PropParent {
  name: string
  fileName: string
}

export interface DefaultValue {
  value: string
  computed: boolean
}

export type OpaqueReason =
  | { type: 'conditionalType' }
  | { type: 'mappedType' }
  | { type: 'moduleAugmentation' }
  | { type: 'runtimeDependent'; functionName: string }
  | { type: 'unresolvableImport'; specifier: string }
  | { type: 'pandaCodegenMissing' }
  | { type: 'depthExceeded' }
  | { type: 'indexedAccess'; expression: string }
  | { type: 'templateLiteral'; expression: string }

export interface ObjectField {
  name: string
  propType: PropType
  required: boolean
  description: string
}

/** PropType is a tagged union via the 'kind' discriminant */
export type PropType =
  | { kind: 'string' }
  | { kind: 'number' }
  | { kind: 'boolean' }
  | { kind: 'null' }
  | { kind: 'undefined' }
  | { kind: 'any' }
  | { kind: 'never' }
  | { kind: 'unknown' }
  | { kind: 'void' }
  | { kind: 'reactNode' }
  | { kind: 'cssProperties' }
  | { kind: 'elementType' }
  | { kind: 'sxProps' }
  | { kind: 'stringLiteral'; value: string }
  | { kind: 'numberLiteral'; value: number }
  | { kind: 'boolLiteral'; value: boolean }
  | { kind: 'union'; members: PropType[] }
  | { kind: 'intersection'; members: PropType[] }
  | { kind: 'array'; element: PropType }
  | { kind: 'tuple'; elements: PropType[] }
  | { kind: 'object'; fields: ObjectField[] }
  | { kind: 'named'; name: string; args: PropType[] }
  | { kind: 'eventHandler'; eventType: string; paramName: string | null }
  | { kind: 'ref'; element: string | null }
  | { kind: 'htmlAttributes'; element: string; omitted: string[] }
  | { kind: 'literalUnion'; members: string[]; hasDefault: boolean }
  | { kind: 'opaque'; raw: string; reason: OpaqueReason }

export interface ParsedProp {
  name: string
  type: PropType
  required: boolean
  defaultValue: DefaultValue | null
  description: string
  tags: Record<string, string>
  parent: PropParent | null
  declarations: PropParent[]
}

export interface ComponentEntry {
  displayName: string
  filePath: string
  description: string
  props: Record<string, ParsedProp>
  inheritance: InheritedLayer[]
  notableInherited: Record<string, ParsedProp>
  discriminantProp: string | null
  composes: string[]
  tags: Record<string, string>
  methods: []
}

export interface EnumEntry {
  name: string
  value: string | number | boolean
  description: string
}

export interface Diagnostic {
  severity: 'error' | 'warning' | 'info'
  message: string
  file?: string
  line?: number
  column?: number
  help?: string
  code: string
}

export interface ExtractionStats {
  componentsExtracted: number
  componentsSkipped: number
  filesParsed: number
  dtsCacheHits: number
  durationMs: number
  tier1Count: number
  tier3Count: number
  opaqueCount: number
}

export interface ExtractionOutput {
  components: Record<string, ComponentEntry>
  enums: Record<string, EnumEntry[]>
  diagnostics: Diagnostic[]
  stats: ExtractionStats
}

export interface IncrementalUpdate {
  updatedComponents: ComponentEntry[]
  affectedFiles: string[]
  diagnostics: Diagnostic[]
  durationMs: number
}

/** Cold extraction — returns JSON string of ExtractionOutput */
export declare function extractAll(options: ExtractOptions): Promise<string>

/** Initialize a watch session with full cold extraction. Returns JSON string of ExtractionOutput. */
export declare function initializeSession(sessionId: number, options: ExtractOptions): Promise<string>

/** Incremental extraction for HMR — returns JSON string of IncrementalUpdate */
export declare function extractFileIncremental(
  filePath: string,
  sessionId: number,
  options: ExtractOptions
): Promise<string>

/** Create a persistent watch session, returns session ID */
export declare function createSession(options: ExtractOptions): number

/** Release a watch session */
export declare function closeSession(sessionId: number): void
