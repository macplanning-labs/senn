/**
 * FilterBar.tsx — Reusable search + filter UI component
 *
 * Provides a consistent, page-agnostic way to render search input
 * with optional filter dropdowns/buttons.
 *
 * Usage:
 *   <FilterBar
 *     searchValue={search}
 *     onSearchChange={setSearch}
 *     searchPlaceholder="Search tickets..."
 *   >
 *     <select value={status} onChange={e => setStatus(e.target.value)}>
 *       <option value="">All Status</option>
 *       <option value="open">Open</option>
 *     </select>
 *   </FilterBar>
 */

import type { ReactNode } from 'react';
import './FilterBar.css';

interface FilterBarProps {
  searchValue: string;
  onSearchChange: (value: string) => void;
  searchPlaceholder?: string;
  children?: ReactNode;
  testId?: string;
}

export function FilterBar({
  searchValue,
  onSearchChange,
  searchPlaceholder = 'Search...',
  children,
  testId = 'filter-bar',
}: FilterBarProps) {
  return (
    <div className="filter-bar" data-testid={testId}>
      <input
        type="text"
        className="filter-bar__search"
        placeholder={searchPlaceholder}
        value={searchValue}
        onChange={(e) => onSearchChange(e.target.value)}
        data-testid="filter-bar-search"
      />
      {children && <div className="filter-bar__filters">{children}</div>}
    </div>
  );
}
