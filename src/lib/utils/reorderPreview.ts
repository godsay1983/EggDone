export interface RowCenter {
  id: number;
  centerY: number;
}

export function movePreviewByPointer(
  orderedIds: number[],
  draggedId: number,
  pointerY: number,
  rowCenters: RowCenter[],
): number[] {
  const currentIndex = orderedIds.indexOf(draggedId);
  if (currentIndex < 0) return orderedIds;

  const centers = new Map(rowCenters.map((row) => [row.id, row.centerY]));
  let nextIndex = currentIndex;

  if (currentIndex > 0) {
    const previousCenter = centers.get(orderedIds[currentIndex - 1]);
    if (previousCenter !== undefined && pointerY < previousCenter) {
      nextIndex = currentIndex - 1;
    }
  }

  if (nextIndex === currentIndex && currentIndex < orderedIds.length - 1) {
    const nextCenter = centers.get(orderedIds[currentIndex + 1]);
    if (nextCenter !== undefined && pointerY > nextCenter) {
      nextIndex = currentIndex + 1;
    }
  }

  if (nextIndex === currentIndex) return orderedIds;

  const reordered = [...orderedIds];
  const [movedId] = reordered.splice(currentIndex, 1);
  reordered.splice(nextIndex, 0, movedId);
  return reordered;
}
