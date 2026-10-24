/**
 * ResizeObserver coalescing window handed to recharts' `ResponsiveContainer`.
 *
 * Without it every layout frame re-renders — and re-animates — every mounted
 * chart: the sidebar collapse is a spring that re-lays-out the content pane for
 * ~60 frames, and a window drag-resize does the same. recharts throttles its
 * ResizeObserver callback by this value (trailing edge), so charts resize a few
 * times per gesture instead of once per frame. The *first* size still comes
 * from a synchronous `getBoundingClientRect()` inside the container, so the
 * initial paint is not delayed and nothing flashes empty.
 *
 * Keep it small: it is a coalescing window, not a "wait for the resize to end"
 * delay — the chart must still feel like it follows the drag.
 */
export const CHART_RESIZE_DEBOUNCE = 100;
