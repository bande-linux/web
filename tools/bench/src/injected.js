(() => {
  const frame = () => new Promise(requestAnimationFrame);
  const report = (values) =>
    fetch('/__report?' + Object.entries(values).map(([k, v]) => k + '=' + v).join('&'));
  const median = (list) => list.slice().sort((a, b) => a - b)[list.length >> 1];
  const percentile = (list, p) =>
    list.slice().sort((a, b) => a - b)[Math.round((list.length - 1) * p)];
  const settings = new URLSearchParams(location.search);
  const warmLeft = Number(settings.get('warm') || 0);
  addEventListener('load', async () => {
    await frame();
    await frame();
    const navigation = performance.getEntriesByType('navigation')[0];
    const paint = performance.getEntriesByType('paint').find((e) => e.name === 'first-contentful-paint');
    const fcp = paint ? paint.startTime : -1;
    await report({
      origin: performance.timeOrigin,
      fcp,
      load: navigation.loadEventEnd || performance.now(),
      phase: 1,
    });
    if (settings.get('report') !== 'full') return;
    if (warmLeft > 0) {
      settings.set('warm', warmLeft - 1);
      if (warmLeft === 1) settings.set('report', 'full');
    }
    if (settings.get('micro') !== '1') {
      const source = await (await fetch(location.href.split('&')[0])).text();
      const timesParse = [];
      for (let i = 0; i < 7; i++) {
        const start = performance.now();
        new DOMParser().parseFromString(source, 'text/html');
        timesParse.push(performance.now() - start);
      }
      const sheet = document.createElement('style');
      sheet.textContent = '.bench-all * { outline-offset: 1px }';
      document.head.append(sheet);
      const root = document.documentElement;
      const timesStyle = [];
      for (let i = 0; i < 7; i++) {
        const start = performance.now();
        root.classList.toggle('bench-all');
        getComputedStyle(document.body).outlineOffset;
        timesStyle.push(performance.now() - start);
      }
      const timesLayout = [];
      for (let i = 0; i < 7; i++) {
        root.style.width = 1264 - (i % 2) * 16 + 'px';
        const start = performance.now();
        root.offsetHeight;
        timesLayout.push(performance.now() - start);
      }
      root.style.width = '';
      await frame();
      const frames = [];
      const range = Math.max(1, root.scrollHeight - innerHeight);
      let last = performance.now();
      for (let i = 0; i < 120; i++) {
        scrollTo(0, (i * 200) % range);
        await frame();
        const now = performance.now();
        frames.push(now - last);
        last = now;
      }
      scrollTo(0, 0);
      await report({
        parse: median(timesParse),
        restyle: median(timesStyle),
        relayout: median(timesLayout),
        scroll50: percentile(frames, 0.5),
        scroll99: percentile(frames, 0.99),
        phase: 2,
      });
      settings.set('micro', '1');
    }
    if (warmLeft > 0) {
      location.search = '?' + settings.toString();
    } else {
      await report({ phase: 3 });
    }
  });
})();
