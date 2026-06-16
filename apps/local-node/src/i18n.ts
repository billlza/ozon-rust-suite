export type Locale = "zh-CN" | "en-US";

type SetupStepKey = "saveOzon" | "connectOpenClaw" | "readProducts";
type LoginStateKey = "connected" | "checking";

const zhCN = {
  language: {
    label: "语言",
    zh: "中文",
    en: "English"
  },
  app: {
    title: "Ozon Rust Local",
    subtitle: "店铺商品与出图助手",
    ready: "ready",
    offline: "offline"
  },
  overview: {
    localService: "本机服务",
    openClawConnection: "龙虾连接",
    pendingApproval: "待确认",
    writePolicy: "写入保护",
    writePolicyValue: "先确认",
    productMode: "商品模式",
    desktopNode: "桌面节点",
    readSchedule: "定时读取",
    scheduleOn: "已开启",
    schedulePaused: "暂停",
    realRead: "真实读取",
    connected: "已连接",
    checking: "检测中",
    restartNode: "重启节点"
  },
  protocol: {
    title: (protocol: string) => `节点协议 ${protocol}`,
    waiting: "等待 /health",
    detail: (version: string, commit: string, supervisor: string) => `版本 ${version} / ${commit} / ${supervisor}`,
    unknownSupervisor: "未知 supervisor",
    pending: "检测通过后这里会显示协议、构建和 supervisor 信息。"
  },
  setup: {
    title: "3 步连接龙虾 / Codex",
    description: "普通用户只需要按顺序完成下面三步。manifest 和工具清单只放在高级诊断里。",
    actions: {
      autoBind: "自动打开龙虾绑定",
      copyPairing: "复制一次性绑定链接",
      recheck: "重新检测"
    },
    note: "自动绑定使用 5 分钟有效的一次性配对码。本机只提供商品和图片事实，不接管用户的龙虾账号。",
    steps: {
      saveOzon: {
        title: "保存店铺授权",
        done: "已完成",
        needLease: "还差电脑授权",
        pending: "待填写",
        detail: "把 Ozon Seller 后台里的 Client ID 和 API Key 粘贴到这里。本机先校验，失败就不会读取商品。"
      },
      connectOpenClaw: {
        title: "连接龙虾 / Codex",
        ready: "可自动绑定",
        waiting: "等待本机节点",
        detail: "打开龙虾绑定页，本机会用一次性配对码交接连接配置。"
      },
      readProducts: {
        title: "读取商品并出图",
        selected: "已选商品",
        ready: "可开始",
        waiting: "等前两步",
        detail: "读取真实商品和图片后，再复制海报任务给龙虾/Codex。"
      }
    } satisfies Record<SetupStepKey, Record<string, string>>
  },
  bridge: {
    title: "龙虾连接信息",
    description: "这里仅用于排查本机 manifest 和工具清单，不再展示长期连接令牌。",
    advancedSummary: "查看 manifest / 工具清单",
    copyManifestTitle: "复制 manifest 地址",
    approvalRequired: "需要本机确认",
    readOnly: "只读"
  },
  config: {
    title: "保存 Ozon 店铺授权",
    description: "把 Ozon Seller 后台里的 Client ID 和 API Key 粘贴到这里。密钥只保存在这台电脑。",
    realModeNotice: "当前是真实读取：未保存或校验失败时不会读取商品。",
    mockModeNotice: "当前是开发演示模式：只适合本机演示，上线必须切到真实读取。",
    clientIdPlaceholder: "从 Ozon Seller 后台复制",
    apiKeyPlaceholder: "保存后只显示指纹",
    saveAndValidate: "保存并检测店铺"
  },
  advanced: {
    summary: "高级设置与诊断",
    imageApiTitle: "可选：图片 API 自动出图",
    imageApiDescription: "龙虾/Codex 能出图时不需要填这里。只有要让本机后台自动生成背景时，才配置官方或中转 API。",
    baseUrlPlaceholder: "https://api.openai.com 或中转地址",
    apiKeyReusePlaceholder: "留空则沿用已保存的 Key",
    apiKeyFirstPlaceholder: "第一次保存需要填写",
    saveImageConfig: "保存出图配置",
    diagnosticsTitle: "本地自检",
    diagnosticsDescription: "确认连接、密钥来源和 Ozon connector 模式。",
    refreshDiagnostics: "刷新自检",
    validateOzonCredentials: "校验 Ozon 凭据",
    labels: {
      connector: "连接器",
      secretStore: "密钥保存",
      ozonConfig: "Ozon 授权",
      clientId: "Client ID",
      apiKeyFingerprint: "API Key 指纹",
      imageApi: "图片 API",
      imageModel: "图片模型",
      lease: "电脑授权"
    },
    values: {
      unknown: "未知",
      realApi: "真实 API",
      mock: "演示",
      available: "可用",
      unavailable: "不可用",
      configured: "已配置",
      notConfigured: "未配置",
      checking: "检测中",
      valid: "有效",
      missing: "缺失"
    },
    notSaved: "未保存",
    notImported: "未导入",
    validationReal: "真实 Ozon 只读凭据校验通过。",
    validationMock: "mock 连接器校验通过；真实读取请使用 OZON_CONNECTOR_MODE=real。"
  },
  read: {
    title: "真实商品读取",
    description: "真实模式直接调用 Ozon Seller API；未配置凭据或授权租约时失败关闭。",
    loadProducts: "读取 Ozon 商品",
    lookupPlaceholder: "输入商品编码，例如 Offer ID",
    readDetails: "读取详情/图片",
    notRead: "未读取",
    archivedBadge: "归档商品",
    archivedNotice: "当前没有读取到在售商品，已显示归档商品。归档商品可以用于历史查看和素材参考，生成海报或发布前请确认商品仍可销售。",
    imageEmpty: "Ozon 没有返回图片。",
    productCount: "商品数量",
    realSellerData: "真实店铺数据",
    notLoaded: "未读取",
    nextCursor: (value: string) => `下一页：${value}`,
    productFallback: (productId: string) => `商品 ${productId}`,
    productMeta: (productId: string, mode: string) => `product ${productId} · ${mode}`,
    yes: "是",
    no: "否",
    archivedShort: "已归档",
    visibilityUnavailable: "可见性未知",
    primaryImage: "主图",
    attributes: "属性",
    barcodes: "条码",
    available: "可用",
    missing: "缺失",
    attributeFallback: "属性",
    emptyValue: "无"
  },
  poster: {
    title: "商品海报工作台",
    description: "先从 Ozon 商品生成事实包，再交给龙虾/Codex 出图；本机 API 自动出图只是备用。",
    buildBrief: "生成文案简报",
    copyHandoff: "复制给龙虾/Codex",
    apiGenerate: "API 自动生成背景",
    verifyCopy: "校验文案一致性",
    apiUnavailable: "API 自动出图暂不可用；可以先复制给龙虾/Codex。",
    headline: "标题",
    subheadline: "副标题",
    sellingPoint: (index: number) => `卖点 ${index}`,
    cta: "收尾一句",
    note: "说明",
    route: "出图路径",
    theme: "主题",
    verification: "校验",
    viaOpenClaw: "龙虾/Codex",
    notGenerated: "未生成",
    passed: "通过",
    needsFix: "待修正",
    notChecked: "未校验",
    copiedTitle: "已复制给龙虾/Codex",
    copiedDescription: (count: number) =>
      `任务包包含商品事实、${count} 张图片 URL 和海报约束；使用龙虾自己的登录账号生成，不需要在本机填写 OpenAI API Key。`,
    copyAgain: "再复制一次",
    backgroundPrompt: "背景提示词",
    mismatch: (field: string, expected: string, actual: string) => `${field}: 期望“${expected}”，当前是“${actual}”`,
    productAlt: "商品",
    themes: {
      studio: "干净棚拍",
      spotlight: "聚光质感",
      launch: "发布会舞台",
      lifestyle: "生活方式"
    }
  },
  schedule: {
    title: "只读定时采集",
    description: "按固定间隔调用官方 Ozon read-only API；OpenClaw 只能提议，不能启用。",
    intervalTitle: "采集间隔秒数",
    limitTitle: "每次读取样本数量",
    enable: "启用",
    stop: "停止",
    runNow: "立即采集",
    status: "状态",
    enabled: "已启用",
    paused: "已暂停",
    lastCount: "上次商品数",
    lastSample: "上次样本数",
    notRun: "未运行"
  },
  tasks: {
    title: "提案、审批、执行",
    description: "OpenClaw 只创建 dry-run 提案，本地操作员审批后才进入执行队列。",
    operations: {
      ozon_update_price_mock: "改价提案",
      ozon_update_inventory_mock: "改库存提案",
      ozon_join_promotion_mock: "参加促销提案",
      draft_upload_mock: "草稿上传预演",
      import1688_mock: "1688 导入预演"
    },
    create: "创建 dry-run",
    empty: "还没有任务。先创建一个 dry-run。",
    approve: "审批",
    execute: "执行 dry-run",
    noWarnings: "没有警告",
    states: {
      pending_approval: "待审批",
      queued: "排队中",
      succeeded: "已完成",
      failed: "失败"
    },
    risks: {
      high: "高风险",
      medium: "中风险",
      low: "低风险"
    }
  },
  sidecar: {
    external: "已连接",
    blocked: "需处理",
    running: "本地节点运行中",
    recovered: (count: number) => `本地节点运行中，已自恢复 ${count} 次`,
    failed: "本地节点启动失败",
    restarting: "本地节点正在重启",
    connected: "本地节点已连接",
    portBlocked: "本地节点端口被占用",
    unknown: "本地节点未确认运行",
    justNow: "刚刚",
    runningDiagnostic: (started: string, logPath: string) => `监听 127.0.0.1:8790 / 17870，启动时间 ${started}，日志 ${logPath}`,
    externalDiagnostic: (logPath: string) => `检测到已有 Ozon Rust Local 节点正在监听 127.0.0.1:8790 / 17870，桌面端已直接连接。日志 ${logPath}`,
    defaultDiagnostic: "桌面端会托管 local-node；若端口被占用或 sidecar 缺失，这里会显示具体错误。",
    logUnavailable: "当前不在桌面应用内，日志不可用",
    existingAgentPortNotReady: (logPath: string) => `127.0.0.1:8790 已有本地节点，但 17870 agent 端口未就绪。日志 ${logPath}`,
    existingTokenRejected: (logPath: string) => `检测到已有本地节点，但当前桌面端 token 无法访问 /config/status。日志 ${logPath}`,
    lastError: (error: string, logPath: string) => `${error} · 日志 ${logPath}`,
    lastExit: (reason: string, logPath: string) => `${reason} · 日志 ${logPath}`
  },
  gates: {
    connectLocalFirst: "先连接本机节点并刷新状态",
    secretStoreUnavailable: "电脑助手暂时不能保存密钥，请重启电脑助手后再试",
    needsLease: "真实 Ozon 读取需要从 ozon66.com 写入有效授权租约",
    useOpenClawWithoutImageApi: "未配置图片 API 时，先用“复制给龙虾/Codex”生成；API 只用于后台自动出图。",
    blocked: (action: string, reason: string) => `${action}被拦截：${reason}`
  },
  actions: {
    readProducts: "读取 Ozon 商品",
    readDetails: "读取商品详情",
    buildPosterBrief: "生成海报简报",
    preparePosterHandoff: "准备龙虾/Codex 海报任务",
    generatePosterBackground: "生成海报背景",
    verifyPosterCopy: "校验海报文案",
    enableSchedule: "启用只读定时采集",
    runNow: "立即采集"
  },
  messages: {
    initial: "本地节点尚未连接",
    unknownError: "未知错误",
    localServiceConnected: "本地服务已连接",
    localServiceNotReady: "本地服务未就绪",
    localServiceUnreachable: "无法连接 127.0.0.1:8790",
    fillOzonCredentials: "请填写真实的 Ozon Client ID 和 API Key",
    saveFailed: (error: string) => `保存失败：${error}`,
    ozonSavedValidationFailed: (error: string) => `Ozon 凭据已保存，但校验没通过：${error}`,
    ozonSavedValidated: (clientId: string, fingerprint: string) => `Ozon 凭据已保存并校验通过：${clientId} / ${fingerprint}`,
    envOpenAiKeyMustBeReentered: "当前 Key 来自启动环境变量。要在界面里修改地址或模型，请重新填写一次 API Key 后保存。",
    firstOpenAiKeyRequired: "第一次保存图片 API 配置需要填写 API Key；以后只改地址或模型可以留空。",
    imageApiSaved: (baseUrl: string, model: string, fingerprint: string) => `图片 API 已保存：${baseUrl} / ${model} / ${fingerprint}`,
    imageApiSaveFailed: (error: string) => `图片 API 保存失败：${error}`,
    ozonValidationFailed: (error: string) => `Ozon 凭据校验失败：${error}`,
    ozonReadFailed: (error: string) => `Ozon 读取失败：${error}`,
    archivedLoaded: (count: number) => `当前店铺没有读取到在售商品，已显示 ${count} 个归档商品。生成海报前请确认商品仍可销售。`,
    productReadReal: (total: number, sample: number) => `真实 Ozon 商品读取完成：总数 ${total}，当前样本 ${sample}`,
    productReadMock: "开发模式 mock 商品读取完成；上线请使用 OZON_CONNECTOR_MODE=real",
    lookupRequired: "请填写一个 offer_id、product_id 或 sku 来读取详情",
    productDetailFailed: (error: string) => `Ozon 商品详情读取失败：${error}`,
    productDetailReal: (offerId: string, imageCount: number) => `真实商品详情读取完成：${offerId}，图片 ${imageCount} 张`,
    productDetailMock: (offerId: string) => `Mock 商品详情读取完成：${offerId}`,
    needProductForBrief: "先读取一个真实商品，再生成海报简报",
    posterBriefFailed: (error: string) => `海报简报生成失败：${error}`,
    posterBriefReady: "海报简报已生成。推荐先复制给龙虾/Codex 出图；配置图片 API 后也可以后台自动生成背景。",
    needProductForHandoff: "先读取一个真实商品，再复制给龙虾/Codex",
    posterHandoffFailed: (error: string) => `龙虾/Codex 任务包生成失败：${error}`,
    posterHandoffCopied: (offerId: string, imageCount: number) => `已复制给龙虾/Codex 的海报任务：${offerId}，包含 ${imageCount} 张商品图。`,
    posterBackgroundBlocked: (reason: string) => `生成海报背景被拦截：${reason}`,
    needProductForBackground: "先读取一个真实商品，再生成海报背景",
    posterBackgroundFailed: (error: string) => `海报背景生成失败：${error}`,
    posterBackgroundReady: (model: string) => `背景图已生成，模型 ${model}`,
    needProductForVerify: "先读取商品，再校验海报文案",
    posterVerifyFailed: (error: string) => `海报校验失败：${error}`,
    posterVerifyPassed: "海报文案已通过系统稿一致性校验",
    posterVerifyFailedCopy: "海报文案和系统稿不一致，请回到商品属性再确认",
    inputLookup: "请输入 offer_id、product_id 或 sku",
    invalidLookup: "查询格式不对：可用 sku:、offer:、product: 前缀，或直接输入 offer_id / 数字 product_id",
    dryRunCreated: "OpenClaw dry-run 提案已创建，等待本地审批",
    dryRunCreateFailed: (error: string) => `创建失败：${error}`,
    approved: "任务已审批并进入队列",
    approveFailed: (error: string) => `审批失败：${error}`,
    executed: "dry-run 执行完成，没有发送真实 Ozon 写操作",
    executeFailed: (error: string) => `执行失败：${error}`,
    scheduleConfigFailed: (error: string) => `定时读取配置失败：${error}`,
    scheduleEnabled: "只读定时采集已启用",
    scheduleStopped: "只读定时采集已停止",
    manualRunFailed: (error: string) => `手动采集失败：${error}`,
    manualRunDone: (sample: number, duration: number) => `采集完成：${sample} 个样本，${duration}ms`,
    manifestCopied: "manifest 地址已复制",
    nodeNotReadyRefresh: "本机节点还没准备好，请先点击刷新或重启节点。",
    pairingLinkFailed: (error: string) => `生成绑定链接失败：${error}`,
    pairingLinkCopied: "一次性绑定链接已复制，5 分钟内有效。不要转发给其他人。",
    nodeNotReadyRecheck: "本机节点还没准备好，请先点击重新检测。",
    autoBindingFailed: (error: string) => `自动绑定启动失败：${error}`,
    bindingRejected: (error: string) => `绑定链接被安全策略拒绝：${error}`,
    bindingLinkCopiedFallback: "系统没有打开浏览器，已复制一次性绑定链接。请手动粘贴到浏览器打开。",
    bindingOpened: "已打开龙虾绑定页。绑定页会用一次性配对码连接这台电脑，不会把长期令牌放进网址。",
    restartingNode: "正在重启本地节点",
    restartRequested: "本地节点已请求重启，正在重新检测",
    restartUnsupported: "当前环境不支持重启本地节点"
  },
  errors: {
    credentialsMissing: "先在“本地密钥”里保存 Ozon Client ID 和 API Key，并完成校验。",
    envCredentialsIncomplete: "Ozon 启动环境里的 Client ID / API Key 不完整，请改为在界面里保存完整凭据。",
    cloudLeaseMissing: "这台电脑还没完成授权。请先回 ozon66.com 授权这台电脑。",
    cloudLeaseInvalid: "电脑授权记录无效。请安装最新版电脑助手，然后回 ozon66.com 重新完成电脑授权。",
    imageModelUnavailable: "当前图片 API 没有这个模型通道。可以先用“复制给龙虾/Codex”出图，或换一个支持 gpt-image-1 / gpt-image-2 的 API Key。",
    openAiKeyRequired: "第一次保存图片 API 配置需要填写 API Key；保存过以后，只改地址或模型可以留空。"
  }
};

const enUS: typeof zhCN = {
  language: {
    label: "Language",
    zh: "中文",
    en: "English"
  },
  app: {
    title: "Ozon Rust Local",
    subtitle: "Product and poster assistant",
    ready: "ready",
    offline: "offline"
  },
  overview: {
    localService: "Local service",
    openClawConnection: "OpenClaw link",
    pendingApproval: "Pending",
    writePolicy: "Write policy",
    writePolicyValue: "Confirm first",
    productMode: "Product mode",
    desktopNode: "Desktop node",
    readSchedule: "Read schedule",
    scheduleOn: "Enabled",
    schedulePaused: "Paused",
    realRead: "Real read",
    connected: "Connected",
    checking: "Checking",
    restartNode: "Restart node"
  },
  protocol: {
    title: (protocol: string) => `Node protocol ${protocol}`,
    waiting: "waiting for /health",
    detail: (version: string, commit: string, supervisor: string) => `Version ${version} / ${commit} / ${supervisor}`,
    unknownSupervisor: "unknown supervisor",
    pending: "Protocol, build, and supervisor details appear here after the check passes."
  },
  setup: {
    title: "Connect OpenClaw / Codex in 3 steps",
    description: "Most users only need these three steps. Manifest and tool lists stay under advanced diagnostics.",
    actions: {
      autoBind: "Open OpenClaw binding",
      copyPairing: "Copy one-time binding link",
      recheck: "Check again"
    },
    note: "Auto binding uses a 5-minute one-time pairing code. This app only provides product facts and images; it does not take over the user's OpenClaw account.",
    steps: {
      saveOzon: {
        title: "Save shop credentials",
        done: "Done",
        needLease: "Needs device authorization",
        pending: "Required",
        detail: "Paste the Client ID and API Key from Ozon Seller. The local node validates first; failed credentials will not read products."
      },
      connectOpenClaw: {
        title: "Connect OpenClaw / Codex",
        ready: "Auto binding ready",
        waiting: "Waiting for local node",
        detail: "Open the binding page and hand off the connection config with a one-time local pairing code."
      },
      readProducts: {
        title: "Read products and make posters",
        selected: "Product selected",
        ready: "Ready",
        waiting: "Waiting for previous steps",
        detail: "Read real product facts and images, then send the poster task to OpenClaw/Codex."
      }
    }
  },
  bridge: {
    title: "OpenClaw connection",
    description: "For diagnostics only: local manifest and tool list. Long-lived tokens are not shown here.",
    advancedSummary: "View manifest / tool list",
    copyManifestTitle: "Copy manifest URL",
    approvalRequired: "Needs local confirmation",
    readOnly: "Read-only"
  },
  config: {
    title: "Save Ozon shop credentials",
    description: "Paste the Client ID and API Key from Ozon Seller. Secrets stay on this computer.",
    realModeNotice: "Real read mode: products will not be read until credentials are saved and validated.",
    mockModeNotice: "Demo mode: local demos only. Production must use real read mode.",
    clientIdPlaceholder: "Copy from Ozon Seller",
    apiKeyPlaceholder: "Only a fingerprint is shown after saving",
    saveAndValidate: "Save and test shop"
  },
  advanced: {
    summary: "Advanced settings and diagnostics",
    imageApiTitle: "Optional: image API background generation",
    imageApiDescription: "Do not fill this in if OpenClaw/Codex can generate images. Configure an official or proxy API only for background generation by this local node.",
    baseUrlPlaceholder: "https://api.openai.com or proxy URL",
    apiKeyReusePlaceholder: "Leave blank to keep the saved key",
    apiKeyFirstPlaceholder: "Required the first time",
    saveImageConfig: "Save image config",
    diagnosticsTitle: "Local diagnostics",
    diagnosticsDescription: "Check connection, secret source, and Ozon connector mode.",
    refreshDiagnostics: "Refresh diagnostics",
    validateOzonCredentials: "Validate Ozon credentials",
    labels: {
      connector: "Connector",
      secretStore: "Secret store",
      ozonConfig: "Ozon config",
      clientId: "Client ID",
      apiKeyFingerprint: "API key fingerprint",
      imageApi: "Image API",
      imageModel: "Image model",
      lease: "Device lease"
    },
    values: {
      unknown: "unknown",
      realApi: "real API",
      mock: "mock",
      available: "available",
      unavailable: "unavailable",
      configured: "configured",
      notConfigured: "not configured",
      checking: "checking",
      valid: "valid",
      missing: "missing"
    },
    notSaved: "Not saved",
    notImported: "Not imported",
    validationReal: "Real Ozon read-only credentials validated.",
    validationMock: "Mock connector validation passed. Use OZON_CONNECTOR_MODE=real for real Ozon reads."
  },
  read: {
    title: "Real product read",
    description: "Real mode calls the Ozon Seller API directly; missing credentials or lease fail closed.",
    loadProducts: "Read Ozon products",
    lookupPlaceholder: "Enter product code, for example Offer ID",
    readDetails: "Read details/images",
    notRead: "Not read",
    archivedBadge: "Archived",
    archivedNotice: "No active products were returned, so archived products are shown. Use archived products for history or reference only, and confirm sale status before poster or publishing work.",
    imageEmpty: "Ozon returned no images.",
    productCount: "Product count",
    realSellerData: "Real seller data",
    notLoaded: "Not loaded",
    nextCursor: (value: string) => `Next: ${value}`,
    productFallback: (productId: string) => `Product ${productId}`,
    productMeta: (productId: string, mode: string) => `product ${productId} · ${mode}`,
    yes: "yes",
    no: "no",
    archivedShort: "archived",
    visibilityUnavailable: "visibility n/a",
    primaryImage: "Primary image",
    attributes: "Attributes",
    barcodes: "Barcodes",
    available: "available",
    missing: "missing",
    attributeFallback: "attribute",
    emptyValue: "n/a"
  },
  poster: {
    title: "Product poster workspace",
    description: "Build a fact package from Ozon first, then hand it to OpenClaw/Codex. Local API generation is a backup path.",
    buildBrief: "Build copy brief",
    copyHandoff: "Copy to OpenClaw/Codex",
    apiGenerate: "Generate background via API",
    verifyCopy: "Verify copy consistency",
    apiUnavailable: "API image generation is unavailable; copy the task to OpenClaw/Codex instead.",
    headline: "Headline",
    subheadline: "Subheadline",
    sellingPoint: (index: number) => `Selling point ${index}`,
    cta: "Closing line",
    note: "Note",
    route: "Image path",
    theme: "Theme",
    verification: "Verification",
    viaOpenClaw: "OpenClaw/Codex",
    notGenerated: "Not generated",
    passed: "Passed",
    needsFix: "Needs changes",
    notChecked: "Not checked",
    copiedTitle: "Copied to OpenClaw/Codex",
    copiedDescription: (count: number) =>
      `The task package includes product facts, ${count} image URL(s), and poster constraints. It uses the user's own OpenClaw account and does not require an OpenAI API key in this local app.`,
    copyAgain: "Copy again",
    backgroundPrompt: "Background prompt",
    mismatch: (field: string, expected: string, actual: string) => `${field}: expected "${expected}", current "${actual}"`,
    productAlt: "product",
    themes: {
      studio: "clean studio",
      spotlight: "spotlight",
      launch: "launch stage",
      lifestyle: "lifestyle"
    }
  },
  schedule: {
    title: "Read-only schedule",
    description: "Calls official Ozon read-only APIs at a fixed interval; OpenClaw can propose, but cannot enable it.",
    intervalTitle: "Collection interval in seconds",
    limitTitle: "Sample count per run",
    enable: "Enable",
    stop: "Stop",
    runNow: "Run now",
    status: "Status",
    enabled: "enabled",
    paused: "paused",
    lastCount: "Last count",
    lastSample: "Last sample",
    notRun: "Not run"
  },
  tasks: {
    title: "Proposals, approval, execution",
    description: "OpenClaw only creates dry-run proposals. A local operator must approve before execution.",
    operations: {
      ozon_update_price_mock: "Price change proposal",
      ozon_update_inventory_mock: "Inventory change proposal",
      ozon_join_promotion_mock: "Promotion proposal",
      draft_upload_mock: "Draft upload rehearsal",
      import1688_mock: "1688 import rehearsal"
    },
    create: "Create dry-run",
    empty: "No tasks yet. Create a dry-run first.",
    approve: "Approve",
    execute: "Execute dry-run",
    noWarnings: "No warnings",
    states: {
      pending_approval: "Pending approval",
      queued: "Queued",
      succeeded: "Succeeded",
      failed: "Failed"
    },
    risks: {
      high: "High risk",
      medium: "Medium risk",
      low: "Low risk"
    }
  },
  sidecar: {
    external: "Connected",
    blocked: "Needs attention",
    running: "Local node is running",
    recovered: (count: number) => `Local node is running; self-recovered ${count} time(s)`,
    failed: "Local node failed to start",
    restarting: "Local node is restarting",
    connected: "Local node connected",
    portBlocked: "Local node port is occupied",
    unknown: "Local node status not confirmed",
    justNow: "just now",
    runningDiagnostic: (started: string, logPath: string) => `Listening on 127.0.0.1:8790 / 17870, started ${started}, log ${logPath}`,
    externalDiagnostic: (logPath: string) => `An existing Ozon Rust Local node is listening on 127.0.0.1:8790 / 17870; the desktop app connected to it. Log ${logPath}`,
    defaultDiagnostic: "The desktop app manages local-node. Port conflicts or missing sidecars appear here with concrete errors.",
    logUnavailable: "Logs are unavailable outside the desktop app",
    existingAgentPortNotReady: (logPath: string) => `An existing local node is on 127.0.0.1:8790, but the 17870 agent port is not ready. Log ${logPath}`,
    existingTokenRejected: (logPath: string) => `An existing local node was detected, but this desktop token cannot access /config/status. Log ${logPath}`,
    lastError: (error: string, logPath: string) => `${error} · log ${logPath}`,
    lastExit: (reason: string, logPath: string) => `${reason} · log ${logPath}`
  },
  gates: {
    connectLocalFirst: "Connect the local node and refresh status first",
    secretStoreUnavailable: "The desktop assistant cannot save secrets right now. Restart it and try again.",
    needsLease: "Real Ozon reads need a valid authorization lease from ozon66.com",
    useOpenClawWithoutImageApi: "No image API is configured. Use “Copy to OpenClaw/Codex” first; the API is only for background generation.",
    blocked: (action: string, reason: string) => `${action} blocked: ${reason}`
  },
  actions: {
    readProducts: "Read Ozon products",
    readDetails: "Read product details",
    buildPosterBrief: "Build poster brief",
    preparePosterHandoff: "Prepare OpenClaw/Codex poster task",
    generatePosterBackground: "Generate poster background",
    verifyPosterCopy: "Verify poster copy",
    enableSchedule: "Enable read-only schedule",
    runNow: "Run now"
  },
  messages: {
    initial: "Local node is not connected",
    unknownError: "Unknown error",
    localServiceConnected: "Local service connected",
    localServiceNotReady: "Local service is not ready",
    localServiceUnreachable: "Cannot connect to 127.0.0.1:8790",
    fillOzonCredentials: "Enter the real Ozon Client ID and API Key",
    saveFailed: (error: string) => `Save failed: ${error}`,
    ozonSavedValidationFailed: (error: string) => `Ozon credentials were saved, but validation failed: ${error}`,
    ozonSavedValidated: (clientId: string, fingerprint: string) => `Ozon credentials saved and validated: ${clientId} / ${fingerprint}`,
    envOpenAiKeyMustBeReentered: "The current key comes from startup environment variables. Re-enter the API key before changing the URL or model in the UI.",
    firstOpenAiKeyRequired: "The first image API save requires an API key. Later URL/model changes may leave it blank.",
    imageApiSaved: (baseUrl: string, model: string, fingerprint: string) => `Image API saved: ${baseUrl} / ${model} / ${fingerprint}`,
    imageApiSaveFailed: (error: string) => `Image API save failed: ${error}`,
    ozonValidationFailed: (error: string) => `Ozon credential validation failed: ${error}`,
    ozonReadFailed: (error: string) => `Ozon read failed: ${error}`,
    archivedLoaded: (count: number) => `No active products were returned; showing ${count} archived product(s). Confirm sale status before using them for posters.`,
    productReadReal: (total: number, sample: number) => `Real Ozon read complete: total ${total}, current sample ${sample}`,
    productReadMock: "Mock product read complete. Production must use OZON_CONNECTOR_MODE=real.",
    lookupRequired: "Enter exactly one offer_id, product_id, or sku to read details",
    productDetailFailed: (error: string) => `Ozon product detail read failed: ${error}`,
    productDetailReal: (offerId: string, imageCount: number) => `Real product detail read complete: ${offerId}, ${imageCount} image(s)`,
    productDetailMock: (offerId: string) => `Mock product detail read complete: ${offerId}`,
    needProductForBrief: "Read a real product before building a poster brief",
    posterBriefFailed: (error: string) => `Poster brief failed: ${error}`,
    posterBriefReady: "Poster brief ready. Copy to OpenClaw/Codex first; background generation is available after image API configuration.",
    needProductForHandoff: "Read a real product before copying to OpenClaw/Codex",
    posterHandoffFailed: (error: string) => `OpenClaw/Codex task package failed: ${error}`,
    posterHandoffCopied: (offerId: string, imageCount: number) => `Poster task copied for OpenClaw/Codex: ${offerId}, ${imageCount} product image(s).`,
    posterBackgroundBlocked: (reason: string) => `Poster background generation blocked: ${reason}`,
    needProductForBackground: "Read a real product before generating a poster background",
    posterBackgroundFailed: (error: string) => `Poster background generation failed: ${error}`,
    posterBackgroundReady: (model: string) => `Background image generated with ${model}`,
    needProductForVerify: "Read a product before verifying poster copy",
    posterVerifyFailed: (error: string) => `Poster verification failed: ${error}`,
    posterVerifyPassed: "Poster copy matches the system brief",
    posterVerifyFailedCopy: "Poster copy does not match the system brief. Recheck the product attributes.",
    inputLookup: "Enter offer_id, product_id, or sku",
    invalidLookup: "Invalid lookup. Use sku:, offer:, product:, or enter a plain offer_id / numeric product_id.",
    dryRunCreated: "OpenClaw dry-run proposal created; waiting for local approval",
    dryRunCreateFailed: (error: string) => `Create failed: ${error}`,
    approved: "Task approved and queued",
    approveFailed: (error: string) => `Approval failed: ${error}`,
    executed: "dry-run completed; no real Ozon write was sent",
    executeFailed: (error: string) => `Execution failed: ${error}`,
    scheduleConfigFailed: (error: string) => `Read schedule config failed: ${error}`,
    scheduleEnabled: "Read-only schedule enabled",
    scheduleStopped: "Read-only schedule stopped",
    manualRunFailed: (error: string) => `Manual run failed: ${error}`,
    manualRunDone: (sample: number, duration: number) => `Collection complete: ${sample} sample(s), ${duration}ms`,
    manifestCopied: "Manifest URL copied",
    nodeNotReadyRefresh: "The local node is not ready. Refresh or restart the node first.",
    pairingLinkFailed: (error: string) => `Binding link generation failed: ${error}`,
    pairingLinkCopied: "One-time binding link copied. It expires in 5 minutes. Do not forward it.",
    nodeNotReadyRecheck: "The local node is not ready. Check again first.",
    autoBindingFailed: (error: string) => `Auto binding failed: ${error}`,
    bindingRejected: (error: string) => `Binding link rejected by safety policy: ${error}`,
    bindingLinkCopiedFallback: "The browser did not open, so the one-time binding link was copied. Paste it into a browser manually.",
    bindingOpened: "OpenClaw binding page opened. It uses a one-time pairing code and does not put long-lived tokens in the URL.",
    restartingNode: "Restarting local node",
    restartRequested: "Local node restart requested; checking again",
    restartUnsupported: "This environment cannot restart the local node"
  },
  errors: {
    credentialsMissing: "Save Ozon Client ID and API Key under shop credentials, then validate them.",
    envCredentialsIncomplete: "The startup Client ID / API Key is incomplete. Save complete credentials in the UI instead.",
    cloudLeaseMissing: "This computer is not authorized yet. Go back to ozon66.com and authorize this computer first.",
    cloudLeaseInvalid: "The computer authorization is invalid. Install the latest desktop assistant, then authorize this computer again on ozon66.com.",
    imageModelUnavailable: "The current image API does not expose this model. Use “Copy to OpenClaw/Codex” first, or use an API key that supports gpt-image-1 / gpt-image-2.",
    openAiKeyRequired: "The first image API save requires an API key. Later URL/model changes may leave it blank."
  }
};

export const localeOptions: Array<{ locale: Locale; label: string }> = [
  { locale: "zh-CN", label: "中文" },
  { locale: "en-US", label: "English" }
];

export function copyFor(locale: Locale) {
  return locale === "en-US" ? enUS : zhCN;
}

export function normalizeLocale(value: string | null | undefined): Locale {
  if (!value) return "zh-CN";
  return value.toLowerCase().startsWith("en") ? "en-US" : "zh-CN";
}

export function initialLocale(): Locale {
  if (typeof window === "undefined") {
    return "zh-CN";
  }
  return normalizeLocale(window.localStorage.getItem("ozon-local-locale") ?? window.navigator.language);
}
