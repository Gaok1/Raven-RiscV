/* =========================================================
   FALCON ASM — Landing Page Scripts  (Red theme)
   ========================================================= */

// ── Nav scroll effect ──────────────────────────────────────
const nav = document.getElementById('nav');
window.addEventListener('scroll', () => {
  nav.classList.toggle('scrolled', window.scrollY > 20);
}, { passive: true });

// ── Hamburger menu ─────────────────────────────────────────
const hamburger = document.getElementById('navHamburger');
const navMobile  = document.getElementById('navMobile');

if (hamburger && navMobile) {
  hamburger.addEventListener('click', () => {
    const open = navMobile.classList.toggle('open');
    hamburger.classList.toggle('open', open);
    hamburger.setAttribute('aria-expanded', open);
    navMobile.setAttribute('aria-hidden', !open);
  });

  // Close when any link inside is tapped
  navMobile.querySelectorAll('a').forEach(link => {
    link.addEventListener('click', () => {
      navMobile.classList.remove('open');
      hamburger.classList.remove('open');
      hamburger.setAttribute('aria-expanded', false);
      navMobile.setAttribute('aria-hidden', true);
    });
  });

  // Close on outside click / scroll
  document.addEventListener('click', (e) => {
    if (navMobile.classList.contains('open') &&
        !navMobile.contains(e.target) &&
        !hamburger.contains(e.target)) {
      navMobile.classList.remove('open');
      hamburger.classList.remove('open');
      hamburger.setAttribute('aria-expanded', false);
      navMobile.setAttribute('aria-hidden', true);
    }
  });
}

// ── Tabs ───────────────────────────────────────────────────
document.querySelectorAll('.tab-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const tab = btn.dataset.tab;
    document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
    btn.classList.add('active');
    document.querySelector(`[data-panel="${tab}"]`).classList.add('active');
  });
});

// ── IntersectionObserver: fade-in on scroll ───────────────
const fadeObserver = new IntersectionObserver((entries) => {
  entries.forEach(entry => {
    if (entry.isIntersecting) {
      entry.target.style.opacity = '1';
      entry.target.style.transform = 'translateY(0)';
      fadeObserver.unobserve(entry.target);
    }
  });
}, { threshold: 0.08, rootMargin: '0px 0px -32px 0px' });

const animTargets = [
  '.feature-card',
  '.pipeline-step',
  '.isa-category',
  '.qs-option',
  '.audience-card',
];

animTargets.forEach(selector => {
  document.querySelectorAll(selector).forEach((el, i) => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(20px)';
    el.style.transition = `opacity 0.5s ease ${i * 0.07}s, transform 0.5s ease ${i * 0.07}s`;
    fadeObserver.observe(el);
  });
});

// ── Highlight active nav link on scroll ───────────────────
const sections = document.querySelectorAll('section[id]');
const navLinks = document.querySelectorAll('.nav-links a[href^="#"]');

const sectionObserver = new IntersectionObserver((entries) => {
  entries.forEach(entry => {
    if (entry.isIntersecting) {
      const id = entry.target.getAttribute('id');
      navLinks.forEach(link => {
        const isActive = link.getAttribute('href') === `#${id}`;
        link.style.color = isActive ? 'var(--red-bright)' : '';
      });
    }
  });
}, { threshold: 0.4 });

sections.forEach(s => sectionObserver.observe(s));

// ── Bit field tooltips ────────────────────────────────────
document.querySelectorAll('.bf').forEach(bf => {
  bf.title =
    bf.querySelector('.bf-label').textContent + ' ' +
    bf.querySelector('.bf-bits').textContent;
});

// ── Parallax: subtle falcon drift on mouse move ───────────
const falconImg = document.querySelector('.falcon-img');
if (falconImg) {
  document.addEventListener('mousemove', (e) => {
    const cx = window.innerWidth  / 2;
    const cy = window.innerHeight / 2;
    const dx = (e.clientX - cx) / cx;  // -1 to 1
    const dy = (e.clientY - cy) / cy;
    falconImg.style.transform =
      `translateY(${-8 + dy * -6}px) rotateY(${dx * 4}deg)`;
  });

  document.addEventListener('mouseleave', () => {
    falconImg.style.transform = '';
  });
}