export const TICKET_TYPES = ['bug', 'issue', 'task', 'qa'] as const;
export type TicketType = (typeof TICKET_TYPES)[number];
