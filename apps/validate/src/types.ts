/** Normalized prop representation — common subset of rdg + rdt output */
export interface NormalizedProp {
  name: string
  required: boolean
  type: string          // raw type string
  description: string
  defaultValue?: string
}

export interface NormalizedComponent {
  displayName: string
  description: string
  props: Record<string, NormalizedProp>
}

export type NormalizedOutput = Record<string, NormalizedComponent>

export interface ToolResult {
  tool: 'react-docgen' | 'react-docgen-typescript' | 'oxc-react-docgen'
  fixture: string
  durationMs: number
  output: NormalizedOutput
  /** HTML element(s) the component inherits from (our tool only) */
  inheritedElements?: string[]
  /** Notable inherited prop names surfaced from the HTML element (our tool only) */
  notableInheritedNames?: string[]
  error?: string
}

export interface ComparisonResult {
  fixture: string
  /** Props in rdt but not in rdg */
  rdtOnly: string[]
  /** Props in rdg but not in rdt */
  rdgOnly: string[]
  /** Props in both but with different types */
  typeDiffs: Array<{ prop: string; rdt: string; rdg: string }>
  /** Components detected by rdt but not rdg */
  rdtOnlyComponents: string[]
  /** Components detected by rdg but not rdt */
  rdgOnlyComponents: string[]
}
