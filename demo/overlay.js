/* ==========================================================================
   GlassMC 在线体验页的外壳脚本
   只在 demo 部署里加载，正式客户端不含这段代码。
   职责：加一条说明栏、把「关窗」这类在浏览器里没有意义的按钮拦下来。
   ========================================================================== */

(function () {
  'use strict';

  var RELEASES = 'https://github.com/shaun-chen2/GlassMC/releases/latest';

  document.title = 'GlassMC · 在线体验';

  function toast(msg) {
    var el = document.createElement('div');
    el.className = 'demo-toast';
    el.textContent = msg;
    document.body.appendChild(el);
    requestAnimationFrame(function () {
      el.classList.add('on');
    });
    setTimeout(function () {
      el.classList.remove('on');
      setTimeout(function () {
        el.remove();
      }, 260);
    }, 2600);
  }

  function mountBar() {
    if (document.querySelector('.demo-bar')) return;

    var bar = document.createElement('div');
    bar.className = 'demo-bar';
    bar.setAttribute('role', 'note');
    bar.innerHTML =
      '<a class="demo-back" href="/">← 介绍页</a>' +
      '<span class="demo-dot"></span>' +
      '<span class="demo-txt"><b>在线体验版</b>' +
      '<span class="demo-more"> · 示例数据，不会真的安装或启动游戏</span></span>' +
      '<a class="demo-cta" href="' + RELEASES + '">下载正式版</a>' +
      '<button class="demo-x" type="button" aria-label="隐藏提示">×</button>';

    document.body.appendChild(bar);
    bar.querySelector('.demo-x').onclick = function () {
      bar.remove();
    };
  }

  function mountGate() {
    if (document.querySelector('.demo-gate')) return;

    var gate = document.createElement('div');
    gate.className = 'demo-gate';
    gate.innerHTML =
      '<div class="gate-card">' +
      '<h3>用电脑打开体验更好</h3>' +
      '<p>这是一台桌面启动器，界面按电脑屏幕排的，手机窄屏会挤成一团。</p>' +
      '<div class="gate-actions">' +
      '<a href="' + RELEASES + '">下载正式版</a>' +
      '<button type="button" class="gate-anyway">还是想看看</button>' +
      '</div></div>';

    document.body.appendChild(gate);
    gate.querySelector('.gate-anyway').onclick = function () {
      document.documentElement.setAttribute('data-demo-open', '');
    };
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () {
      mountBar();
      mountGate();
    });
  } else {
    mountBar();
    mountGate();
  }

  // 标题栏的最小化 / 最大化 / 关闭在浏览器里没有对应行为，
  // 直接在捕获阶段拦下，免得点了没反应像坏了一样。
  document.addEventListener(
    'click',
    function (e) {
      var t = e.target;
      var btn = t && t.closest ? t.closest('#btn-min, #btn-max, #btn-close') : null;
      if (!btn) return;
      e.preventDefault();
      e.stopPropagation();
      toast('在线体验版没有真实窗口，去下载正式版试试');
    },
    true
  );
})();
