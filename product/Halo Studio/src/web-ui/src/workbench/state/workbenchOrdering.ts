/**
 * Strip ordering primitives (M5, ADR-0076; issue #57).
 *
 * niri 第一原则的纯函数层：条带顺序是 append-only 的，唯一的变更方式是
 * 「新建任务在焦点（或条带末尾）右侧插入」。这里不存在任何重排 API ——
 * 测试与类型共同保证既有列的相对次序与宽度语义永不改变。
 */

/**
 * Inserts `taskId` into the strip order right after `insertAfterTaskId`.
 *
 * - `insertAfterTaskId === null`（或不在条带中）→ 追加到条带末尾（最右）。
 * - 幂等：taskId 已在条带中时返回原次序的拷贝，绝不移动既有列。
 */
export function insertTaskIntoStrip(
  order: readonly string[],
  taskId: string,
  insertAfterTaskId: string | null,
): string[] {
  if (order.includes(taskId)) {
    return [...order];
  }
  const anchorIndex = insertAfterTaskId === null
    ? -1
    : order.indexOf(insertAfterTaskId);
  if (anchorIndex < 0) {
    return [...order, taskId];
  }
  const insertionAt = anchorIndex + 1;
  return [...order.slice(0, insertionAt), taskId, ...order.slice(insertionAt)];
}

/**
 * Removes a task from the strip order (session teardown only). Neighbours
 * close the gap rigidly; relative order is untouched.
 */
export function removeTaskFromStrip(
  order: readonly string[],
  taskId: string,
): string[] {
  return order.filter(id => id !== taskId);
}

/**
 * Clamped focus movement along the strip. Never wraps: the strip extends to
 * the right forever (press `n` to keep appending), so ArrowRight stops at the
 * rightmost column instead of looping.
 */
export function moveStripFocus(
  order: readonly string[],
  currentTaskId: string | null,
  delta: 1 | -1,
): string | null {
  if (order.length === 0) return null;
  if (currentTaskId === null) {
    return delta === 1 ? order[0] : order[order.length - 1];
  }
  const currentIndex = order.indexOf(currentTaskId);
  if (currentIndex < 0) return order[0];
  const nextIndex = Math.min(
    Math.max(currentIndex + delta, 0),
    order.length - 1,
  );
  return order[nextIndex];
}

/**
 * Resolves the insertion anchor for "new task right of focus". Falls back to
 * appending at the right edge when the focused task is unknown.
 */
export function resolveInsertionAnchor(
  order: readonly string[],
  focusedTaskId: string | null,
): string | null {
  if (focusedTaskId !== null && order.includes(focusedTaskId)) {
    return focusedTaskId;
  }
  return order.length > 0 ? order[order.length - 1] : null;
}
