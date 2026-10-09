export type AnalysisLocale = 'en' | 'zh';

export type CatalogKind = 'table' | 'view';

export type RelationOrigin = 'physical' | 'view' | 'inferred';

export type RelationStrength = 'strong' | 'weak';

export type Cardinality = 'one_to_one' | 'one_to_many' | 'many_to_one' | 'many_to_many';

export type Confidence = 'high' | 'medium' | 'low';

export type ReasonCode = 'foreign-key' | 'view-reference' | 'junction' | 'inferred';

export type AnalysisWarningCode = 'truncated' | 'empty' | 'comments' | 'relations';

export interface SchemaScope {
	connectionId: string;
	connectionName: string;
	engine: string;
	schema: string;
	oracleVersion?: string | null;
}

export interface CatalogColumn {
	name: string;
	dataType: string;
	key: string;
	comment: string;
	ordinal: number;
	inferredComment?: string;
	inferenceConfidence?: Confidence;
}

export interface CatalogObject {
	id: string;
	connectionId: string;
	connectionName: string;
	engine: string;
	schema: string;
	name: string;
	kind: CatalogKind;
	comment: string;
	definition: string;
	columns: CatalogColumn[];
	external: boolean;
	inferredComment?: string;
	inferenceConfidence?: Confidence;
}

export interface ForeignKeyConstraint {
	id: string;
	name: string;
	connectionId: string;
	fromId: string;
	toId: string;
	pairs: Array<{ fromColumn: string; toColumn: string }>;
}

export interface AnalysisWarning {
	code: AnalysisWarningCode;
	connectionName: string;
	schema: string;
	detail?: string;
}

export interface SchemaCatalog {
	objects: CatalogObject[];
	foreignKeys: ForeignKeyConstraint[];
	warnings: AnalysisWarning[];
}

export interface InferredComment {
	objectId: string;
	columnName?: string;
	comment: string;
	confidence: Confidence;
}

export interface InferredRelation {
	fromId: string;
	toId: string;
	fromColumns: string[];
	toColumns: string[];
	strength: RelationStrength;
	cardinality: Cardinality;
	reason: string;
	confidence: Confidence;
}

export interface RelationshipEdge {
	id: string;
	fromId: string;
	toId: string;
	fromColumns: string[];
	toColumns: string[];
	origin: RelationOrigin;
	strength: RelationStrength;
	cardinality: Cardinality;
	reasonCode: ReasonCode;
	reason: string;
	confidence?: Confidence;
	viaId?: string;
}

export interface RelationshipGraph {
	nodes: CatalogObject[];
	edges: RelationshipEdge[];
	warnings: AnalysisWarning[];
}

export interface GraphFilters {
	physical: boolean;
	inferred: boolean;
	strong: boolean;
	weak: boolean;
}

export function objectId(connectionId: string, schema: string, name: string): string {
	return `${connectionId}::${schema}::${name}`;
}
