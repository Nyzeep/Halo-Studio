/**
 * FlowChat virtual scroller — the @tanstack/react-virtual engine behind
 * VirtualMessageList (M4 虚拟化收敛, ADR-0077; issue #53).
 *
 * react-virtuoso was removed from the dependency tree; this module reimplements
 * the exact surface of Virtuoso that VirtualMessageList relies on, on top of
 * `useVirtualizer` from @tanstack/react-virtual:
 *
 *   - `data` + `computeItemKey` + `itemContent` (index translation matches
 *     Virtuoso: both receive the ABSOLUTE item index, i.e. local index +
 *     `firstItemIndex`);
 *   - `firstItemIndex` prepend anchoring: when the prop decreases (older
 *     history prepended), the adapter shifts scrollTop by the inserted
 *     content height so the viewport stays anchored (Virtuoso semantics);
 *   - `initialTopMostItemIndex` applied once per mount after the first
 *     measurement pass;
 *   - `overscan` / `increaseViewportBy` (px, Virtuoso-style) translated into
 *     tanstack's item-count overscan;
 *   - `atBottomThreshold` + `atBottomStateChange` derived from scroll/resize
 *     observation;
 *   - `rangeChanged` derived from the virtualizer's rendered range;
 *   - `defaultItemHeight` + per-index `heightEstimates` as estimateSize;
 *   - `scrollerRef` callback + `context` + `components.Header/Footer`;
 *   - imperative handle: `scrollTo(options)` (native scroller scrollTo, so
 *     VirtualMessageList's wrapper interception still applies) and
 *     `scrollToIndex({ index, align, behavior })` (absolute index, Virtuoso
 *     convention).
 *
 * Not implemented (unused by VirtualMessageList): followOutput (the component
 * passes false and owns bottom tracking itself) and alignToBottom.
 *
 * Item resize corrections: tanstack's default adjusts scrollTop whenever any
 * item ABOVE the viewport edge resizes, which would fight this list's
 * collapse/pin/follow stabilization (FLOWCHAT_SCROLL_STABILITY contract).
 * The adapter narrows it to items that end fully above the scroll offset:
 * content above the viewport keeps the reading anchor, while streaming
 * growth at the tail stays owned by VirtualMessageList.
 */

import React, {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  useVirtualizer,
  type VirtualItem,
  type Virtualizer,
} from '@tanstack/react-virtual';

export type FlowChatScrollAlign = 'start' | 'center' | 'end' | 'auto';

export interface FlowChatVirtualScrollerRange {
  startIndex: number;
  endIndex: number;
}

export interface FlowChatVirtualScrollerContextProp<TContext> {
  context: TContext;
}

export interface FlowChatVirtualScrollerComponents<TContext> {
  Header?: React.ComponentType<FlowChatVirtualScrollerContextProp<TContext>>;
  Footer?: React.ComponentType<FlowChatVirtualScrollerContextProp<TContext>>;
}

/** Methods VirtualMessageList invokes on the handle (Virtuoso-compatible). */
export interface FlowChatVirtualScrollerHandle {
  scrollTo: (options: ScrollToOptions) => void;
  scrollToIndex: (options: {
    index: number;
    align?: FlowChatScrollAlign;
    behavior?: ScrollBehavior;
  }) => void;
}

export interface FlowChatVirtualScrollerProps<TData, TContext = unknown> {
  data: readonly TData[];
  computeItemKey?: (absoluteIndex: number, item: TData) => string | number;
  itemContent?: (absoluteIndex: number, item: TData) => React.ReactNode;
  firstItemIndex?: number;
  initialTopMostItemIndex?: number | { index: number; align?: FlowChatScrollAlign };
  /** px rendered beyond the viewport, Virtuoso-style. */
  overscan?: number | { main: number; reverse: number };
  /** px rendered beyond the viewport on each side, Virtuoso-style. */
  increaseViewportBy?: number | { top: number; bottom: number };
  atBottomThreshold?: number;
  atBottomStateChange?: (atBottom: boolean) => void;
  rangeChanged?: (range: FlowChatVirtualScrollerRange) => void;
  defaultItemHeight?: number;
  /** Per-item initial height estimates (Virtuoso heightEstimates). */
  heightEstimates?: readonly number[];
  scrollerRef?: (element: HTMLElement | null) => void;
  context?: TContext;
  components?: FlowChatVirtualScrollerComponents<TContext>;
  className?: string;
  style?: React.CSSProperties;
}

type ScrollerVirtualizer = Virtualizer<HTMLDivElement, Element>;

const FALLBACK_MEAN_ITEM_SIZE_PX = 96;
const MIN_OVERSCAN_ITEMS = 1;
const MAX_OVERSCAN_ITEMS = 48;

function resolveOverscanPx(
  overscan: FlowChatVirtualScrollerProps<unknown>['overscan'],
  increaseViewportBy: FlowChatVirtualScrollerProps<unknown>['increaseViewportBy'],
): { top: number; bottom: number } {
  const overscanTop = typeof overscan === 'number' ? overscan : overscan?.reverse ?? 0;
  const overscanBottom = typeof overscan === 'number' ? overscan : overscan?.main ?? 0;
  const viewportTop = typeof increaseViewportBy === 'number'
    ? increaseViewportBy
    : increaseViewportBy?.top ?? 0;
  const viewportBottom = typeof increaseViewportBy === 'number'
    ? increaseViewportBy
    : increaseViewportBy?.bottom ?? 0;
  return {
    top: overscanTop + viewportTop,
    bottom: overscanBottom + viewportBottom,
  };
}

const FlowChatVirtualScrollerInner = forwardRef<
  FlowChatVirtualScrollerHandle,
  FlowChatVirtualScrollerProps<unknown, unknown>
>(function FlowChatVirtualScrollerInner(props, ref) {
  const {
    data,
    computeItemKey,
    itemContent,
    firstItemIndex = 0,
    initialTopMostItemIndex,
    overscan,
    increaseViewportBy,
    atBottomThreshold = 4,
    atBottomStateChange,
    rangeChanged,
    defaultItemHeight = 64,
    heightEstimates,
    scrollerRef,
    context,
    components,
    className,
    style,
  } = props;

  const scrollerElementRef = useRef<HTMLDivElement | null>(null);
  const headerWrapperRef = useRef<HTMLDivElement | null>(null);
  const firstItemIndexRef = useRef(firstItemIndex);
  const previousFirstItemIndexRef = useRef(firstItemIndex);
  const atBottomRef = useRef(true);
  const didInitialScrollRef = useRef(false);
  const lastRangeRef = useRef<FlowChatVirtualScrollerRange | null>(null);
  const atBottomStateChangeRef = useRef(atBottomStateChange);
  const rangeChangedRef = useRef(rangeChanged);
  const [scrollMargin, setScrollMargin] = useState(0);

  firstItemIndexRef.current = firstItemIndex;
  atBottomStateChangeRef.current = atBottomStateChange;
  rangeChangedRef.current = rangeChanged;

  const getItemKey = useCallback((index: number) => {
    const item = data[index];
    if (item === undefined) {
      return index;
    }
    return computeItemKey ? computeItemKey(index + firstItemIndexRef.current, item) : index;
  }, [data, computeItemKey]);

  const estimateSize = useCallback((index: number) => (
    heightEstimates?.[index] ?? defaultItemHeight
  ), [heightEstimates, defaultItemHeight]);

  const overscanPx = useMemo(
    () => resolveOverscanPx(overscan, increaseViewportBy),
    [overscan, increaseViewportBy],
  );
  const meanItemSize = Math.max(24, defaultItemHeight || FALLBACK_MEAN_ITEM_SIZE_PX);
  const overscanItems = Math.max(
    MIN_OVERSCAN_ITEMS,
    Math.min(
      MAX_OVERSCAN_ITEMS,
      Math.ceil(Math.max(overscanPx.top, overscanPx.bottom) / meanItemSize),
    ),
  );

  const virtualizer = useVirtualizer({
    count: data.length,
    getScrollElement: () => scrollerElementRef.current,
    estimateSize,
    getItemKey,
    overscan: overscanItems,
    scrollMargin,
  });

  // Narrows tanstack's item-resize scroll correction to items that end fully
  // above the scroll offset (see module comment). `scrollOffset` mirrors
  // instance.getScrollOffset() with initialOffset 0.
  useLayoutEffect(() => {
    virtualizer.shouldAdjustScrollPositionOnItemSizeChange = (item, _delta, instance) => (
      item.end < (instance.scrollOffset ?? 0)
    );
  }, [virtualizer]);

  const notifyAtBottomState = useCallback(() => {
    const scroller = scrollerElementRef.current;
    if (!scroller) {
      return;
    }
    const distanceFromBottom = Math.max(
      0,
      scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop,
    );
    const atBottom = distanceFromBottom <= atBottomThreshold;
    if (atBottom !== atBottomRef.current) {
      atBottomRef.current = atBottom;
      atBottomStateChangeRef.current?.(atBottom);
    }
  }, [atBottomThreshold]);

  const handleVirtualizerChange = useCallback((instance: ScrollerVirtualizer) => {
    const range = instance.range;
    if (range) {
      const nextRange = { startIndex: range.startIndex, endIndex: range.endIndex };
      const previousRange = lastRangeRef.current;
      if (
        !previousRange ||
        previousRange.startIndex !== nextRange.startIndex ||
        previousRange.endIndex !== nextRange.endIndex
      ) {
        lastRangeRef.current = nextRange;
        rangeChangedRef.current?.(nextRange);
      }
    }
    notifyAtBottomState();
  }, [notifyAtBottomState]);

  // Propagate rendered-range changes from every virtualizer notification
  // (scroll, measurement, resize).
  useLayoutEffect(() => {
    handleVirtualizerChange(virtualizer);
  });

  const handleScrollerElement = useCallback((element: HTMLDivElement | null) => {
    scrollerElementRef.current = element;
    // React invokes the previous callback ref with null on unmount/ref swap,
    // which releases VirtualMessageList's wrapped scroller state.
    scrollerRef?.(element);
  }, [scrollerRef]);

  // Track the header block (fixed 57px header + transient boundary status) as
  // scrollMargin so item offsets and scrollToIndex targets account for the
  // content rendered above the list inside the scroller.
  useLayoutEffect(() => {
    const headerElement = headerWrapperRef.current;
    if (!headerElement || typeof ResizeObserver === 'undefined') {
      return;
    }
    const update = () => {
      const height = headerElement.offsetHeight;
      setScrollMargin(current => (current === height ? current : height));
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(headerElement);
    return () => observer.disconnect();
  }, []);

  // initialTopMostItemIndex: applied once per mount (Virtuoso semantics); the
  // parent remounts this component per session and when switching between the
  // static initial-history path and the virtualized path.
  useLayoutEffect(() => {
    if (didInitialScrollRef.current || data.length === 0) {
      return;
    }
    if (initialTopMostItemIndex === undefined) {
      return;
    }
    const target = typeof initialTopMostItemIndex === 'number'
      ? { index: initialTopMostItemIndex, align: 'auto' as FlowChatScrollAlign }
      : { index: initialTopMostItemIndex.index, align: initialTopMostItemIndex.align ?? 'auto' as FlowChatScrollAlign };
    const localIndex = target.index - firstItemIndexRef.current;
    if (localIndex < 0 || localIndex >= data.length) {
      return;
    }
    didInitialScrollRef.current = true;
    virtualizer.scrollToIndex(localIndex, {
      align: target.align,
      behavior: 'auto',
    });
  }, [data.length, initialTopMostItemIndex, virtualizer]);

  // firstItemIndex prepend anchoring: when the host prepends older history,
  // shift the viewport by the inserted content height so the visible anchor
  // stays put (Virtuoso firstItemIndex contract).
  useLayoutEffect(() => {
    const previous = previousFirstItemIndexRef.current;
    previousFirstItemIndexRef.current = firstItemIndex;
    if (firstItemIndex >= previous) {
      return;
    }
    const prepended = previous - firstItemIndex;
    const scroller = scrollerElementRef.current;
    if (!scroller || prepended <= 0) {
      return;
    }
    const measurements = virtualizer.measurementsCache;
    let insertedHeightPx = 0;
    for (let index = 0; index < Math.min(prepended, measurements.length); index += 1) {
      insertedHeightPx += measurements[index]?.size ?? 0;
    }
    if (insertedHeightPx > 0) {
      scroller.scrollTop += insertedHeightPx;
    }
  }, [firstItemIndex, virtualizer]);

  useImperativeHandle(ref, () => ({
    scrollTo: (options: ScrollToOptions) => {
      // Route through the scroller element so VirtualMessageList's scrollTo
      // wrapper (suppression diagnostics) stays the single interception point.
      scrollerElementRef.current?.scrollTo(options);
    },
    scrollToIndex: (options: {
      index: number;
      align?: FlowChatScrollAlign;
      behavior?: ScrollBehavior;
    }) => {
      const localIndex = options.index - firstItemIndexRef.current;
      virtualizer.scrollToIndex(localIndex, {
        align: options.align ?? 'auto',
        behavior: options.behavior ?? 'auto',
      });
    },
  }), [virtualizer]);

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();
  const Header = components?.Header;
  const Footer = components?.Footer;

  return (
    <div
      ref={handleScrollerElement}
      className={className}
      style={{
        width: '100%',
        height: '100%',
        overflowY: 'auto',
        overflowX: 'hidden',
        position: 'relative',
        ...style,
      }}
      data-virtuoso-scroller="true"
      data-testid="virtuoso"
      tabIndex={0}
    >
      <div ref={headerWrapperRef}>
        {Header ? <Header context={context} /> : null}
      </div>
      <div
        data-virtual-items-container="true"
        style={{
          height: `${totalSize}px`,
          position: 'relative',
          width: '100%',
        }}
      >
        {virtualItems.map((virtualItem: VirtualItem) => {
          const item = data[virtualItem.index];
          return (
            <div
              key={virtualItem.key}
              ref={virtualizer.measureElement}
              data-index={virtualItem.index}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualItem.start - scrollMargin}px)`,
              }}
            >
              {item !== undefined && itemContent
                ? itemContent(virtualItem.index + firstItemIndexRef.current, item)
                : null}
            </div>
          );
        })}
      </div>
      {Footer ? <Footer context={context} /> : null}
    </div>
  );
});

/**
 * Typed wrapper so consumers keep full generic inference on data/context while
 * the internal implementation deals with the erased types above.
 */
export const FlowChatVirtualScroller = FlowChatVirtualScrollerInner as unknown as <TData, TContext = unknown>(
  props: FlowChatVirtualScrollerProps<TData, TContext> & { ref?: React.Ref<FlowChatVirtualScrollerHandle> },
) => React.ReactElement | null;
