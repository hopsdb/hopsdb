import { useCallback, useEffect, useRef, useState } from "react";
import { getTableList, getTreeChildren } from "../api";
import type { TreeNode } from "../types";

/**
 * Split "NAME(COMMENT)" (Oracle column/table comments) into plain name + comment.
 * Returns just { name, comment } when matched, or { name: original } otherwise.
 */
function splitNameComment(raw: string): { name: string; comment?: string } {
  const m = /^(.+?)\((.+)\)$/.exec(raw);
  if (m) {
    const name = m[1] ?? "";
    const comment = m[2]?.trim();
    return comment ? { name, comment } : { name };
  }
  return { name: raw };
}

/**
 * Object categories shown in the tree, per database flavor.
 *
 * Oracle has PACKAGE / PACKAGE BODY and no separate ROUTINES node; MySQL and
 * PostgreSQL have ROUTINES (functions/procedures) but no Oracle-style packages.
 * PostgreSQL also has standard sequences; MySQL omits that category because
 * plain MySQL has no portable sequence catalog.
 * "TABLE" is marked `special: "tables"` because its children load lazily:
 * the table list on connect, columns on expand.
 */
interface Category {
  id: string;
  label: string;
  type: "SEQUENCE" | "PACKAGE" | "PACKAGE BODY" | "VIEW" | "ROUTINE" | "INDEX" | "TABLE";
  special?: "tables";
}

const CATEGORIES: Record<string, Category[]> = {
  oracle: [
    { id: "seq", label: "SEQUENCE", type: "SEQUENCE" },
    { id: "pkg", label: "PACKAGE", type: "PACKAGE" },
    { id: "pkgb", label: "PACKAGE BODY", type: "PACKAGE BODY" },
    { id: "view", label: "VIEWS", type: "VIEW" },
    { id: "tbl", label: "TABLES", type: "TABLE", special: "tables" },
  ],
  mysql: [
    { id: "tbl", label: "TABLES", type: "TABLE", special: "tables" },
    { id: "view", label: "VIEWS", type: "VIEW" },
    { id: "routine", label: "ROUTINES", type: "ROUTINE" },
  ],
  postgresql: [
    { id: "tbl", label: "TABLES", type: "TABLE", special: "tables" },
    { id: "view", label: "VIEWS", type: "VIEW" },
    { id: "routine", label: "ROUTINES", type: "ROUTINE" },
    { id: "seq", label: "SEQUENCES", type: "SEQUENCE" },
  ],
};

function buildInitialTree(dbType: string): TreeNode {
  const key = dbType.toLowerCase();
  // Normalize common alias so we always hit one of the CATEGORIES keys.
  const flavorKey = key === "postgres" ? "postgresql" : key;
  const flavor = CATEGORIES[flavorKey] ?? CATEGORIES.oracle;
  return {
    id: "root",
    label: flavorKey === "oracle" ? "OBJECTS" : "SCHEMA",
    children: flavor.map((c) => ({
      id: c.id,
      label: c.label,
      type: c.type,
      ...(c.special === "tables" ? { children: [] } : {}),
    })),
  };
}

interface Props {
  /** Double-click a leaf node (PACKAGE / PACKAGE BODY → view source) */
  onDoubleClick: (type: string, name: string) => void;
  /** Session change (rebuild after reconnect) */
  sessionKey: string;
  /** Backend session id for this workspace */
  sessionId: string;
  /** Database flavor for tree query templates */
  dbType: string;
  /** Global running lock shared with SQL execution */
  globalBusy?: boolean;
  /** Notify parent when tree is loading data */
  onBusyChange?: (busy: boolean) => void;
  /** Only auto-load tables after SQL environment is ready */
  autoLoadTables?: boolean;
  /** Called once the table list finishes loading (used by MainWindow to return
   * focus to the SQL editor). */
  onTablesLoaded?: () => void;
}

/** Left object tree — expandable schema/object browser */
export default function ObjectTree({
  onDoubleClick,
  sessionKey,
  sessionId,
  dbType,
  globalBusy = false,
  onBusyChange,
  autoLoadTables = false,
  onTablesLoaded,
}: Props) {
  const [tree, setTree] = useState<TreeNode>(() => buildInitialTree(dbType));
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [tableLoaded, setTableLoaded] = useState(false);
  const [error, setError] = useState("");
  const loadingCountRef = useRef(0);
  const [shouldLoadTables, setShouldLoadTables] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Rebuild on session change
  useEffect(() => {
    setTree(buildInitialTree(dbType));
    setExpanded(new Set());
    setTableLoaded(false);
    setError("");
    setShouldLoadTables(false);
    loadingCountRef.current = 0;
    setSelectedId(null);
  }, [sessionKey, dbType]);

  const beginBusy = useCallback(() => {
    loadingCountRef.current += 1;
    onBusyChange?.(true);
  }, [loadingCountRef, onBusyChange]);

  const endBusy = useCallback(() => {
    loadingCountRef.current = Math.max(0, loadingCountRef.current - 1);
    if (loadingCountRef.current === 0) {
      onBusyChange?.(false);
    }
  }, [loadingCountRef, onBusyChange]);

  useEffect(() => {
    if (autoLoadTables) {
      console.info("[hopsdb] object tree auto-load tables armed", {
        sessionKey,
        dbType,
      });
      setShouldLoadTables(true);
    }
  }, [autoLoadTables, sessionKey, dbType]);

  useEffect(() => {
    if (shouldLoadTables && !tableLoaded) {
      void (async () => {
        console.info("[hopsdb] object tree loading tables", {
          sessionKey,
          dbType,
          shouldLoadTables,
          tableLoaded,
        });
        beginBusy();
        setLoading((prev) => new Set(prev).add("tbl"));
        try {
          const tables = await getTableList(sessionId, dbType);
          console.info("[hopsdb] object tree tables loaded", {
            sessionKey,
            dbType,
            count: tables.length,
            sample: tables.slice(0, 5),
          });
          setTree((prev) => {
            const tblNode = prev.children?.find((c) => c.id === "tbl");
            if (!tblNode) return prev;
            tblNode.children = tables.map((t) => {
              return {
                id: `tbl:${t.name}`,
                label: t.name,
                type: "TABLE",
                ...(t.comment ? { comment: t.comment } : {}),
                children: [],
              };
            });
            return { ...prev };
          });
          setTableLoaded(true);
          onTablesLoaded?.();
          // Auto-expand the TABLES node so the loaded tables are visible immediately.
          setExpanded((prev) => {
            const next = new Set(prev);
            next.add("tbl");
            return next;
          });
        } catch (e) {
          console.info("[hopsdb] object tree load tables failed", {
            sessionKey,
            dbType,
            error: e instanceof Error ? e.message : String(e),
          });
          setError(e instanceof Error ? e.message : String(e));
        } finally {
          console.info("[hopsdb] object tree load tables finished", {
            sessionKey,
            dbType,
          });
          endBusy();
          setLoading((prev) => {
            const next = new Set(prev);
            next.delete("tbl");
            return next;
          });
        }
      })();
    }
  }, [beginBusy, shouldLoadTables, dbType, endBusy, sessionId, sessionKey, tableLoaded]);

  /** Load on expand (lazy-load children) */
  const onExpand = useCallback(
    async (node: TreeNode) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        next.add(node.id);
        return next;
      });

      // TABLE node first expand: load the table list
      if (node.id === "tbl" && !tableLoaded && !autoLoadTables) {
        console.info("[hopsdb] object tree manual table load", {
          sessionKey,
          dbType,
          nodeId: node.id,
        });
        beginBusy();
        setLoading((prev) => new Set(prev).add(node.id));
        try {
          const tables = await getTableList(sessionId, dbType);
          console.info("[hopsdb] object tree manual tables loaded", {
            sessionKey,
            dbType,
            count: tables.length,
          });
          setTree((prev) => {
            const tblNode = prev.children?.find((c) => c.id === "tbl");
            if (!tblNode) return prev;
            tblNode.children = tables.map((t) => {
              return {
                id: `tbl:${t.name}`,
                label: t.name,
                type: "TABLE",
                ...(t.comment ? { comment: t.comment } : {}),
                children: [],
              };
            });
            return { ...prev };
          });
          setTableLoaded(true);
        } catch (e) {
          console.info("[hopsdb] object tree manual table load failed", {
            sessionKey,
            dbType,
            error: e instanceof Error ? e.message : String(e),
          });
          setError(e instanceof Error ? e.message : String(e));
        } finally {
          console.info("[hopsdb] object tree manual table load finished", {
            sessionKey,
            dbType,
          });
          endBusy();
          setLoading((prev) => {
            const next = new Set(prev);
            next.delete(node.id);
            return next;
          });
        }
      }

      // Lazy-load other node types
      if (node.type && node.type !== "TABLE" && !node.children?.length) {
        console.info("[hopsdb] object tree loading children", {
          sessionKey,
          dbType,
          nodeType: node.type,
          nodeLabel: node.label,
        });
        beginBusy();
        setLoading((prev) => new Set(prev).add(node.id));
        try {
          // INDEXES under a table queries by the owning table name.
          const queryName = node.ownerTable || node.label;
          const children = await getTreeChildren(sessionId, node.type, queryName, dbType);
          console.info("[hopsdb] object tree children loaded", {
            sessionKey,
            dbType,
            nodeType: node.type,
            nodeLabel: node.label,
            count: children.length,
          });
          setTree((prev) => {
            const found = findNode(prev, node.id);
            if (!found) return prev;
            found.children = children.map((c) => {
              if (node.type === "INDEX") {
                const [indexName, tableName] = c.split("|");
                return {
                  id: `${node.id}:${indexName}`,
                  label: tableName ? `${indexName} (${tableName})` : indexName,
                };
              }
              return {
                id: `${node.id}:${c}`,
                label: c,
              };
            });
            return { ...prev };
          });
        } catch (e) {
          console.info("[hopsdb] object tree children load failed", {
            sessionKey,
            dbType,
            nodeType: node.type,
            nodeLabel: node.label,
            error: e instanceof Error ? e.message : String(e),
          });
          setError(e instanceof Error ? e.message : String(e));
        } finally {
          console.info("[hopsdb] object tree children load finished", {
            sessionKey,
            dbType,
            nodeType: node.type,
            nodeLabel: node.label,
          });
          endBusy();
          setLoading((prev) => {
            const next = new Set(prev);
            next.delete(node.id);
            return next;
          });
        }
      }

      // Table under TABLE node: load columns on expand
      if (node.type === "TABLE" && node.id.startsWith("tbl:")) {
        console.info("[hopsdb] object tree loading table columns", {
          sessionKey,
          dbType,
          table: node.label,
        });
        beginBusy();
        setLoading((prev) => new Set(prev).add(node.id));
        try {
          const cols = await getTreeChildren(sessionId, "TABLE", node.label, dbType);
          console.info("[hopsdb] object tree table columns loaded", {
            sessionKey,
            dbType,
            table: node.label,
            count: cols.length,
          });
          setTree((prev) => {
            const found = findNode(prev, node.id);
            if (!found) return prev;
            // Surface the owning table's indexes as a collapsible node above
            // the columns (INDEXES -> expand to db-specific index details).
            const childNodes: TreeNode[] = cols.map((raw) => {
              const { name, comment } = splitNameComment(raw);
              return {
                id: `${node.id}:col:${name}`,
                label: name,
                ...(comment ? { comment } : {}),
              };
            });
            childNodes.unshift({
              id: `${node.id}:idx`,
              label: "INDEXES",
              type: "INDEXES",
              ownerTable: node.label,
              children: [],
            });
            found.children = childNodes;
            return { ...prev };
          });
        } catch (e) {
          console.info("[hopsdb] object tree table columns load failed", {
            sessionKey,
            dbType,
            table: node.label,
            error: e instanceof Error ? e.message : String(e),
          });
          setError(e instanceof Error ? e.message : String(e));
        } finally {
          console.info("[hopsdb] object tree table columns load finished", {
            sessionKey,
            dbType,
            table: node.label,
          });
          endBusy();
          setLoading((prev) => {
            const next = new Set(prev);
            next.delete(node.id);
            return next;
          });
        }
      }
    },
    [tableLoaded, dbType, sessionId],
  );

  /** Collapse */
  const onCollapse = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
  };

  /** Double-click: expandable nodes toggle; leaf nodes open source when applicable */
  const onDblClick = useCallback(
    (node: TreeNode, isExp: boolean) => {
      if (node.type || (node.children?.length ?? 0) > 0) {
        // Branch node: toggle expand/collapse
        isExp ? onCollapse(node.id) : onExpand(node);
        return;
      }
      // Leaf node: open source if it's a known object type
      if (node.id.startsWith("pkg:") || node.id.startsWith("pkgb:")) {
        const parentType = node.id.startsWith("pkgb:") ? "PACKAGE BODY" : "PACKAGE";
        onDoubleClick(parentType, node.label);
      } else if (node.id.startsWith("seq:")) {
        onDoubleClick("SEQUENCE", node.label);
      } else if (node.id.startsWith("view:")) {
        onDoubleClick("VIEW", node.label);
      } else if (node.id.startsWith("routine:")) {
        onDoubleClick("ROUTINE", node.label);
      } else if (node.id.startsWith("index:")) {
        onDoubleClick("INDEX", node.id.slice("index:".length));
      }
    },
    [onCollapse, onExpand, onDoubleClick],
  );

  function renderNodes(nodes: TreeNode[] | undefined, depth: number) {
    if (!nodes?.length) return null;
      return nodes.map((node) => {
        const isExpanded = expanded.has(node.id);
        const isLoading = loading.has(node.id);
        const hasChildren = !!node.children?.length || !!node.type;
        const disabled = globalBusy || isLoading;
        const isSelected = selectedId === node.id;
        return (
          <div key={node.id}>
            <div
            className={`tree-node ${depth === 1 ? "tree-category" : ""} ${disabled ? "is-disabled" : ""} ${isSelected ? "is-selected" : ""} ${node.type === "INDEXES" ? "tree-indexes" : ""}`}
            style={{ paddingLeft: 8 + depth * 14 }}
            onClick={(e) => {
              if (disabled) return;
              e.stopPropagation();
              // Single click: highlight the node AND auto-select its text so
              // the user can copy right away with Ctrl/Cmd+C.
              setSelectedId(node.id);
              const label = (e.currentTarget as HTMLElement).querySelector(".tree-label");
              if (label) {
                const range = document.createRange();
                range.selectNodeContents(label);
                const sel = window.getSelection();
                sel?.removeAllRanges();
                sel?.addRange(range);
              }
            }}
            onDoubleClick={() => {
              if (disabled) return;
              onDblClick(node, isExpanded);
            }}
            title={node.label}
          >
            <span className="tree-arrow">{hasChildren ? (isExpanded ? "▾" : "▸") : "·"}</span>
            {isLoading && <span className="tree-spinner">⟳</span>}
            <span className="tree-label" draggable={!hasChildren} onDragStart={(e) => {
              e.dataTransfer.setData("text/plain", node.label);
            }}>
              {node.label}
            </span>
            {node.comment && <span className="tree-comment">{node.comment}</span>}
          </div>
          {isExpanded && renderNodes(node.children, depth + 1)}
        </div>
      );
    });
  }

  return (
    <div className="object-tree">
      <div
        className="tree-header"
        title="Double-click to collapse all branches"
        onDoubleClick={() => setExpanded((prev) => (prev.size === 0 ? prev : new Set()))}
      >
        <span>Objects</span>
        <span className="tree-header-tag">{dbType}</span>
      </div>
      {dbType.toLowerCase() !== "oracle" && (
        <div className="tree-note">
          Showing a database-specific object layout. Table content is loaded from the current
          session, while other groups are kept aligned to the selected tag.
        </div>
      )}
      {error && <div className="tree-error">{error}</div>}
      {renderNodes(tree.children, 0)}
    </div>
  );
}

/** Find a node by id in the tree (mutates in place) */
function findNode(node: TreeNode, id: string): TreeNode | null {
  if (node.id === id) return node;
  for (const child of node.children ?? []) {
    const found = findNode(child, id);
    if (found) return found;
  }
  return null;
}
