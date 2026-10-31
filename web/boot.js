// 最后启动：等所有模块的函数都定义好再 boot。
bindPan("cmpA");
bindPan("cmpB");
boot()
  .then(() => {
    if (window.workspaceRestore) window.workspaceRestore();
  })
  .catch((e) => toast("初始化失败：" + e, true));
renderQueue();
