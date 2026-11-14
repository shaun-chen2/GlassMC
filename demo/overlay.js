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

  /* ---------------------------------- 壁纸慢速随机漂移
     目的不是「有动画」，而是让壁纸动起来 —— 玻璃窗口后面有东西在缓慢移动，
     blur 才看得出来。所以幅度小、周期长：一次位移四十秒往上，
     且还没走完就换下一个目标，动作永远不落停，也不会「一顿一顿」。 */
  var root = document.documentElement;
  var reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var driftTimer = 0;

  function drift() {
    clearTimeout(driftTimer);
    if (reduce) {
      root.style.setProperty('--demo-wall-x', '0px');
      root.style.setProperty('--demo-wall-y', '0px');
      root.style.setProperty('--demo-wall-rot', '0deg');
      return;
    }

    // 幅度取短边的 12%（1440×900 上约 ±103px）。
    // 但壁纸层只比视口大 9vmax，窄高视口（如 820×1000）按短边算会超出这个余量，
    // 所以再按 inset 夹一次，留出旋转与亚像素的余量。
    var w = window.innerWidth;
    var h = window.innerHeight;
    var inset = Math.max(w, h) * 0.09;                 // 与 CSS 的 inset: -9vmax 对齐
    var amp = Math.min(Math.min(w, h) * 0.12, inset - 26);
    var x = (Math.random() * 2 - 1) * amp;
    var y = (Math.random() * 2 - 1) * amp;
    var rot = (Math.random() * 2 - 1) * 0.7;          // 轻微旋转，避免纯平移的机械感
    var dur = 52 + Math.random() * 34;                // 52~86 秒走到目标点，约 1.5px/s

    root.style.setProperty('--demo-wall-dur', dur.toFixed(1) + 's');
    root.style.setProperty('--demo-wall-x', x.toFixed(1) + 'px');
    root.style.setProperty('--demo-wall-y', y.toFixed(1) + 'px');
    root.style.setProperty('--demo-wall-rot', rot.toFixed(2) + 'deg');

    // 提前 15% 换目标：过渡还没结束就改方向，运动是连续的，不会走走停停
    driftTimer = setTimeout(drift, dur * 850);
  }

  drift();
  window.addEventListener('resize', function () {
    // 视口变了重新取一次幅度，免得窄屏上位移过大露出边缘
    drift();
  });
})();
