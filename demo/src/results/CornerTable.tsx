import { Disclosure, Table, type Column } from "@vitavision/ui";

import type { CornerRow } from "./model";

const fixed = (digits: number) => (value: number) => value.toFixed(digits);

export function CornerTable({
  rows,
  selectedId,
  onSelect,
}: {
  rows: readonly CornerRow[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const hasGrid = rows.some((row) => row.grid !== null);
  const hasTargetId = rows.some((row) => row.targetId !== null);
  const hasSigma = rows.some((row) => row.sigma !== null);

  const columns: Column<CornerRow>[] = [
    {
      key: "index",
      header: "#",
      numeric: true,
      cell: (row) => row.index,
      sortValue: (row) => row.index,
      sortable: true,
    },
    ...(hasGrid
      ? [
          {
            key: "grid",
            header: "(i, j)",
            cell: (row: CornerRow) => (row.grid ? `${row.grid.i}, ${row.grid.j}` : ""),
            sortValue: (row: CornerRow) => (row.grid ? row.grid.j * 10_000 + row.grid.i : null),
            sortable: true,
          },
        ]
      : []),
    ...(hasTargetId
      ? [
          {
            key: "id",
            header: "id",
            numeric: true,
            cell: (row: CornerRow) => row.targetId ?? "",
            sortValue: (row: CornerRow) => row.targetId,
            sortable: true,
          },
        ]
      : []),
    {
      key: "score",
      header: "score",
      numeric: true,
      cell: (row) => fixed(1)(row.score),
      sortValue: (row) => row.score,
      sortable: true,
    },
    ...(hasSigma
      ? [
          {
            key: "sigma",
            header: "σ max",
            numeric: true,
            cell: (row: CornerRow) => (row.sigma === null ? "" : fixed(3)(row.sigma)),
            sortValue: (row: CornerRow) => row.sigma,
            sortable: true,
          },
        ]
      : []),
  ];

  return (
    <Disclosure summary="Corner table" count={rows.length}>
      <Table
        caption="Detected corners"
        columns={columns}
        rows={[...rows]}
        rowKey={(row) => row.id}
        isRowActive={(row) => row.id === selectedId}
        onRowClick={(row) => onSelect(row.id)}
        windowed={{ rowHeight: 24, height: 216 }}
        selectionFollowsFocus
      />
    </Disclosure>
  );
}
