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
// CSRF トークン取得（AJAX用）
// ====================================================================
function getCsrfToken() {
  // meta tag から取得（Cookie名に依存しない）
  var meta = document.querySelector('meta[name="csrf-token"]');
  if (meta) return meta.getAttribute('content');
  // フォールバック: hidden input から取得
  var input = document.querySelector('[name=csrfmiddlewaretoken]');
  return input ? input.value : '';
}

// ====================================================================
// 通知ベル（Backlog風ドロップダウン通知）
// ====================================================================
function toggleNotifDropdown() {
  var dd = document.getElementById('notifDropdown');
  if (!dd) return;
  if (dd.classList.contains('show')) {
    dd.classList.remove('show');
  } else {
    dd.classList.add('show');
    fetchNotifications();
  }
}

function fetchNotifications() {
  fetch('/notifications/api/')
    .then(function(r) { return r.json(); })
    .then(function(data) {
      updateBadge(data.unread_count);
      renderNotifications(data.notifications);
    })
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

function renderNotifications(notifications) {
  var body = document.getElementById('notifDropdownBody');
  if (!body) return;

  if (!notifications || notifications.length === 0) {
    body.innerHTML = '<div class="notif-empty">通知はありません</div>';
    return;
  }

  var html = '';
  for (var i = 0; i < notifications.length; i++) {
    var n = notifications[i];
    var url = n.ticket_id
      ? '/notifications/' + n.id + '/read/?next=/tickets/' + n.ticket_id + '/'
      : '#';
    var cls = n.is_read ? 'notif-item' : 'notif-item unread';
    html += '<a class="' + cls + '" href="' + url + '" data-notif-id="' + n.id + '">'
      + '<span class="notif-item-icon">' + n.icon + '</span>'
      + '<div class="notif-item-content">'
      + '<div class="notif-item-title">' + escapeHtml(n.title) + '</div>';
    if (n.message) {
      html += '<div class="notif-item-message">' + escapeHtml(n.message) + '</div>';
    }
    html += '<div class="notif-item-time">' + n.created_at + '</div>'
      + '</div>';
    if (!n.is_read) {
      html += '<div class="notif-unread-dot"></div>';
    }
    html += '</a>';
  }
  body.innerHTML = html;
}

function escapeHtml(str) {
  var div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function markNotifRead(id) {
  // バッジを即デクリメント
  var badge = document.getElementById('notifBadge');
  if (badge) {
    var count = parseInt(badge.textContent) || 0;
    updateBadge(Math.max(0, count - 1));
  }
  // 未読ドットを消す
  var item = document.querySelector('[data-notif-id="' + id + '"]');
  if (item) {
    item.classList.remove('unread');
    var dot = item.querySelector('.notif-unread-dot');
    if (dot) dot.remove();
  }
  // sendBeacon はページ遷移中でも確実に送信される
  var formData = new FormData();
  formData.append('csrfmiddlewaretoken', getCsrfToken());
  navigator.sendBeacon('/notifications/' + id + '/read/', formData);
}

function markAllRead() {
  fetch('/notifications/read-all/', {
    method: 'POST',
    headers: {
      'X-CSRFToken': getCsrfToken(),
      'X-Requested-With': 'XMLHttpRequest',
    },
  }).then(function() {
    updateBadge(0);
    var body = document.getElementById('notifDropdownBody');
    if (body) {
      body.querySelectorAll('.notif-item.unread').forEach(function(el) {
        el.classList.remove('unread');
      });
      body.querySelectorAll('.notif-unread-dot').forEach(function(el) {
        el.remove();
      });
    }
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
    fetchNotifications();
    setInterval(function() {
      fetch('/notifications/api/')
        .then(function(r) { return r.json(); })
        .then(function(data) { updateBadge(data.unread_count); })
        .catch(function() {});
    }, 30000);
  }
});

