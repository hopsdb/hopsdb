import { useCallback, useRef } from "react";

interface ResizerProps {
  /** "v" = vertical divider (adjusts left/right), "h" horizontal (top/bottom) */
  direction: "v" | "h";
  /** Called once when a drag starts (use to snapshot the starting size). */
  onDragStart?: () => void;
  /** Fires on each pointer move with the cumulative delta from the drag start. */
  onDrag: (delta: number) => void;
  className?: string;
}

/**
 * A thin draggable divider. On mousedown it records the start pointer position,
 * fires onDragStart, then fires onDrag with the cumulative delta from the start
 * of the drag on each move. Use  startSize + delta  (not incremental adds) so
 * the divider tracks the mouse 1:1.
 */
export default function Resizer({ direction, onDrag, onDragStart, className }: ResizerProps) {
  const startPosRef = useRef<number | null>(null);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const pos = direction === "h" ? e.clientY : e.clientX;
      startPosRef.current = pos;
      onDragStart?.();

      const onMove = (ev: MouseEvent) => {
        if (startPosRef.current == null) return;
        const cur = direction === "h" ? ev.clientY : ev.clientX;
        onDrag(cur - startPosRef.current);
      };
      const onUp = () => {
        startPosRef.current = null;
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        document.body.style.cursor = "";
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
      document.body.style.cursor = direction === "h" ? "row-resize" : "col-resize";
    },
    [direction, onDrag, onDragStart],
  );

  return (
    <div
      className={`resizer ${direction === "h" ? "resizer-h" : "resizer-v"} ${className ?? ""}`}
      onMouseDown={onMouseDown}
    />
  );
}