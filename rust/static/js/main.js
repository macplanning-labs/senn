/**
 * プロジェクト管理ツール — main.js
 *
 * 設計指針準拠:
 *   5.1 二度押し防止（全POSTフォーム）
 *   5.2 ユーザーフィードバック（アラート自動消去、トースト）
 *   5.3 アクセシビリティ（フォーカス管理）
 *   5.5 セキュリティ（CSRF）
 */

document.addEventListener('DOMContentLoaded', function () {
  initDoubleSubmitGuard();
  initAlertAutoClose();
  initSidebarToggle();
});

// ====================================================================
// 5.1 二度押し防止（設計指針 必須）
// - form[method="post"] に対するグローバルハンドラで一括適用
// - data-no-guard 属性を持つフォームは除外
// - 5秒後に自動復帰（サーバーエラー時の救済）
// ====================================================================
function initDoubleSubmitGuard() {
  document.querySelectorAll('form[method="post"]').forEach(function (form) {
    if (form.dataset.noGuard !== undefined) return;

    form.addEventListener('submit', function (e) {
      if (form.dataset.submitting === 'true') {
        e.preventDefault();
        return;
      }
      form.dataset.submitting = 'true';

      // ボタンを disabled にしスピナー表示
      const submitBtn = form.querySelector('[type="submit"]');
      if (submitBtn) {
        submitBtn.disabled = true;
        submitBtn.dataset.originalText = submitBtn.textContent;
        submitBtn.textContent = '処理中...';
        submitBtn.setAttribute('aria-busy', 'true');
      }

      // フォーム全体の操作を遮断
      form.style.pointerEvents = 'none';
      form.style.opacity = '0.7';

      // 5秒後に自動復帰（画面遷移しなかった場合の救済）
      setTimeout(function () {
        form.dataset.submitting = 'false';
        form.style.pointerEvents = '';
        form.style.opacity = '';
        if (submitBtn) {
          submitBtn.disabled = false;
          submitBtn.textContent = submitBtn.dataset.originalText || '送信';
          submitBtn.removeAttribute('aria-busy');
        }
      }, 5000);
    });
  });
}

// ====================================================================
// 5.2 アラート自動消去（5秒後）
// ====================================================================
function initAlertAutoClose() {
  document.querySelectorAll('.alert').forEach(function (alert) {
    setTimeout(function () {
      alert.style.transition = 'opacity 0.3s ease';
      alert.style.opacity = '0';
      setTimeout(function () { alert.remove(); }, 300);
    }, 5000);
  });
}

// ====================================================================
// トースト通知
// ====================================================================
function showToast(msg, type) {
  const el = document.getElementById('toast');
  if (!el) return;
  el.textContent = msg;
  el.className = 'toast show ' + (type || '');
  setTimeout(function () { el.classList.remove('show'); }, 3000);
}

// ====================================================================
// サイドバー（モバイル対応）
// ====================================================================
function initSidebarToggle() {
  // ESC キーでサイドバーを閉じる
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape') {
      const sidebar = document.getElementById('appSidebar');
      if (sidebar) sidebar.classList.remove('open');
    }
  });
}

function toggleSidebar() {
  var layout = document.querySelector('.app-layout');
  if (layout) {
    layout.classList.toggle('sidebar-collapsed');
    localStorage.setItem('sidebarCollapsed', layout.classList.contains('sidebar-collapsed') ? '1' : '0');
  }
}
// 起動時にサイドバー状態を復元
(function() {
  if (localStorage.getItem('sidebarCollapsed') === '1') {
    var layout = document.querySelector('.app-layout');
    if (layout) layout.classList.add('sidebar-collapsed');
  }
})();

// ====================================================================
// 破壊的操作の確認ダイアログ（設計指針 5.2）
// ====================================================================
function confirmAction(message) {
  return confirm(message);
}

// ====================================================================
// 通知ベル（Backlog風ドロップダウン通知）
// サーバーレンダリング（Cookieセッション）のHTMLルートを叩く。
// ====================================================================
function toggleNotifDropdown() {
  var dd = document.getElementById('notifDropdown');
  if (!dd) return;
  if (dd.classList.contains('show')) {
    dd.classList.remove('show');
  } else {
    dd.classList.add('show');
    refreshNotifDropdownBody();
  }
}

function refreshNotifDropdownBody() {
  fetch('/notifications/dropdown')
    .then(function(r) { return r.text(); })
    .then(function(html) {
      var body = document.getElementById('notifDropdownBody');
      if (body) body.innerHTML = html;
    })
    .catch(function() {});
}

function refreshUnreadBadge() {
  fetch('/notifications/unread-count')
    .then(function(r) { return r.text(); })
    .then(function(text) { updateBadge(parseInt(text, 10) || 0); })
    .catch(function() {});
}

function updateBadge(count) {
  var badge = document.getElementById('notifBadge');
  if (!badge) return;
  if (count > 0) {
    badge.textContent = count > 99 ? '99+' : count;
    badge.style.display = 'flex';
  } else {
    badge.style.display = 'none';
  }
}

function markAllRead() {
  fetch('/notifications/read-all', {
    method: 'POST',
    headers: { 'X-Requested-With': 'XMLHttpRequest' },
  }).then(function() {
    updateBadge(0);
    refreshNotifDropdownBody();
  }).catch(function() {});
}

// ドロップダウン外クリックで閉じる
document.addEventListener('click', function(e) {
  var wrapper = document.getElementById('notifBellWrapper');
  if (wrapper && !wrapper.contains(e.target)) {
    var dd = document.getElementById('notifDropdown');
    if (dd) dd.classList.remove('show');
  }
});

// 初回ロード時と30秒ごとに未読数を更新
document.addEventListener('DOMContentLoaded', function() {
  // 認証済みの場合のみ実行
  if (document.getElementById('notifBadge')) {
    refreshUnreadBadge();
    setInterval(refreshUnreadBadge, 30000);
  }
});

