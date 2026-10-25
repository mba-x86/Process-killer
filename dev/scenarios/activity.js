const lines = (T) => T.$$('#page .ln');
export default [
  { name: 'entries render; live entry appends; Clear view empties; Open log file', async run(T) {
    await T.open('activity');
    await T.waitFor(() => lines(T).length === 2, { msg: '2 entries' });
    T.has(T.text(lines(T)[0]), 'INFO'); T.has(T.text(lines(T)[0]), 'Started in 0.8s'); T.has(T.text(lines(T)[1]), 'SLOW');
    T.assert(!T.text('#page').includes('Nothing logged yet'), 'no empty state');
    await T.step('live entry', async () => {
      T.emit('activity:entry', { ts: Math.floor(Date.now() / 1000), level: 'ok', message: 'Ended brave.exe (pid 123)' });
      T.emit('activity:entry', { ts: Math.floor(Date.now() / 1000), level: 'fail', message: 'Could not stop Spooler' });
      await T.waitFor(() => lines(T).length === 4, { msg: 'live rows' }); T.has(T.text(lines(T)[2]), 'OK'); T.has(T.text(lines(T)[3]), 'FAIL');
    });
    await T.step('Clear view', async () => {
      const m = T.mark(); await T.clickText('Clear view');
      T.eq(lines(T).length, 0); T.has(T.text('#page'), 'Nothing logged yet'); T.eq(T.calls('clear_activity', m).length, 1);
      T.emit('activity:entry', { ts: 1, level: 'info', message: 'after clear' });
      await T.waitFor(() => lines(T).length === 1 && !T.text('#page').includes('Nothing logged yet'), { msg: 'row after clear' });
    });
    await T.step('Open log file', async () => { const m = T.mark(); await T.clickText('Open log file'); T.eq(T.calls('open_log_file', m).length, 1); });
    await T.step('leaving unlistens', async () => { await T.nav('dashboard'); await T.sleep(100); T.eq(T.listeners('activity:entry'), 0, 'activity:entry listener leaked'); });
  } },
  { name: 'leaving before the log loads does not leak a listener', async run(T) {
    await T.open('dashboard');
    T.win.location.hash = '#/activity';
    await T.sleep(5);
    T.win.location.hash = '#/dashboard';
    await T.waitFor(() => T.title() === 'Dashboard', { msg: 'back on dashboard' });
    await T.sleep(1500);
    T.eq(T.listeners('activity:entry'), 0, 'activity:entry listener leaked after fast navigation');
  } },
];
