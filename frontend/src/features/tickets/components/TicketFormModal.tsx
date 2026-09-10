/**
 * TicketFormModal.tsx — チケット作成モーダル
 *
 * どのページからでもチケットを作成できるよう、ルート遷移ではなく
 * オーバーレイで TicketForm を表示する。
 *
 * 見た目は中央寄せのモーダルだが、背景の暗幕(overlay)は
 * pointer-events:none にしてクリックを透過させている。
 * これにより一覧やチケット詳細パネルの操作をブロックしないため、
 * 作成中の下書き（説明文・添付画像）を保持したまま他チケットを
 * 参照できる（overlayクリックでの閉じる動作も、背景を操作可能にする
 * 都合上あえて実装していない。閉じるのは✕ボタンのみ）。
 */
import { useUIStore } from '@/shared/stores/uiStore';
import { TicketForm } from './TicketForm';
import './TicketFormModal.css';

export function TicketFormModal() {
  const {
    ticketFormModalOpen,
    ticketFormModalProjectKey,
    ticketFormModalTeamSlug,
    ticketFormModalInitialDescription,
    ticketFormModalInitialParent,
    closeTicketFormModal,
  } = useUIStore();

  if (!ticketFormModalOpen || (!ticketFormModalProjectKey && !ticketFormModalTeamSlug)) return null;

  return (
    <div className="ticket-form-modal__overlay">
      <div className="ticket-form-modal__dialog" role="dialog" aria-label="チケット作成">
        <button
          type="button"
          className="ticket-form-modal__close"
          onClick={closeTicketFormModal}
          aria-label="Close"
        >
          ✕
        </button>
        <TicketForm
          projectKeyOverride={ticketFormModalProjectKey ?? undefined}
          teamSlugOverride={ticketFormModalTeamSlug ?? undefined}
          initialDescription={ticketFormModalInitialDescription ?? undefined}
          initialParent={ticketFormModalInitialParent ?? undefined}
          onClose={closeTicketFormModal}
        />
      </div>
    </div>
  );
}
