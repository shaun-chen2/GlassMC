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
     目的不是「有动画」，而是让玻璃窗后面有东西在动 —— 有东西动，才看得出毛玻璃。
     上一版每秒只挪 1~2px、幅度 ±100px：数值上确实在动，肉眼基本看不出来。
     这一版改成随机游走：
       每次目标 90~170px，22~34 秒走完（约 4~11px/s），没走完就换下一个目标，
     所以永远在缓慢移动、又不会「一顿一顿」。用真实元素 + inline transform，
     不依赖 var() 驱动 transform，Safari 上也一样会动。 */
  var wall = document.createElement('div');
  wall.className = 'demo-wall';
  var host = document.body || document.documentElement;
  host.insertBefore(wall, host.firstChild);

  var reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var SCALE = 1.12;
  var pos = { x: 0, y: 0, rot: 0 };
  var lim = { x: 120, y: 100 };
  var driftTimer = 0;

  // 安全边界：壁纸层 inset 是 -12vmax，再放大 SCALE，剩下的余量才是能移动的行程。
  // 旋转会把四个角甩出去，甩出的距离随元素对角线增长（大屏上能到 40px），
  // 所以这里按实际尺寸算，不能用固定值。
  function measure() {
    var w = window.innerWidth;
    var h = window.innerHeight;
    var inset = Math.max(w, h) * 0.12;                 // 与 CSS 的 inset: -12vmax 对齐
    var ew = w + inset * 2;
    var eh = h + inset * 2;
    var rotSlack = Math.sqrt(ew * ew + eh * eh) / 2 * Math.sin(1.2 * Math.PI / 180) + 8;
    lim.x = Math.max(40, (ew * SCALE - w) / 2 - rotSlack);
    lim.y = Math.max(30, (eh * SCALE - h) / 2 - rotSlack);
  }

  function apply(duration) {
    wall.style.transition = duration > 0
      ? 'transform ' + duration.toFixed(1) + 's cubic-bezier(.42, .03, .36, 1)'
      : 'none';
    wall.style.transform =
      'translate3d(' + pos.x.toFixed(1) + 'px,' + pos.y.toFixed(1) + 'px,0) ' +
      'rotate(' + pos.rot.toFixed(2) + 'deg) scale(' + SCALE + ')';
  }

  function drift() {
    var w = window.innerWidth;
    var h = window.innerHeight;
    var ang = Math.random() * Math.PI * 2;
    var dist = Math.min(w, h) * (0.10 + Math.random() * 0.09);   // 90~170px @900
    var nx = pos.x + Math.cos(ang) * dist;
    var ny = pos.y + Math.sin(ang) * dist;

    // 撞到边界就折返到对侧，路径不会在角落里反复蹭
    if (Math.abs(nx) > lim.x) nx = -nx * (0.45 + Math.random() * 0.35);
    if (Math.abs(ny) > lim.y) ny = -ny * (0.45 + Math.random() * 0.35);
    pos.x = Math.max(-lim.x, Math.min(lim.x, nx));
    pos.y = Math.max(-lim.y, Math.min(lim.y, ny));
    pos.rot = -1.1 + Math.random() * 2.2;

    var dur = 22 + Math.random() * 12;                 // 22~34 秒，慢但不至于看不出来
    apply(dur);
    // 82% 就换目标：过渡还没结束就改方向，看上去是一直在飘，不会走走停停
    clearTimeout(driftTimer);
    driftTimer = setTimeout(drift, dur * 820);
  }

  measure();
  apply(0);
  if (!reduce) {
    driftTimer = setTimeout(drift, 200);
  }

  window.addEventListener('resize', function () {
    measure();
    pos.x = Math.max(-lim.x, Math.min(lim.x, pos.x));
    pos.y = Math.max(-lim.y, Math.min(lim.y, pos.y));
    apply(0.8);
  });
})();
