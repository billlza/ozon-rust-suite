import { useCallback, useEffect, useMemo, useState } from "react";

export type Locale = "zh" | "en";

const LOCALE_STORAGE_KEY = "ozon-rust-suite.portal.locale";
const supportedLocales: Locale[] = ["zh", "en"];

const zh = {
  common: {
    brand: "Ozon Rust Suite",
    details: {
      collapse: "收起",
      expand: "展开"
    },
    language: {
      ariaLabel: "切换语言",
      zh: "中文",
      en: "English"
    }
  },
  portal: {
    nav: {
      primaryAria: "主导航",
      loggedIn: {
        guide: "接入向导",
        manual: "操作说明",
        troubleshoot: "排查"
      },
      public: {
        capabilities: "功能",
        workflow: "流程",
        pricing: "方案",
        manual: "操作说明"
      },
      actions: {
        refresh: "刷新",
        logout: "退出",
        login: "登录",
        register: "注册"
      }
    },
    defaults: {
      deviceName: "我的电脑",
      releasePending: "待同步",
      releaseChecksumPending: "等待 release-manifest.json",
      downloadSyncPending: "等待下载信息同步",
      currentModel: "当前模型"
    },
    hero: {
      eyebrow: "Ozon Rust Suite",
      titleLine1: "商品图进来，",
      titleLine2: "海报成稿出去。",
      description: "读取真实商品图、标题和卖点，本机助手负责店铺授权，龙虾/Codex 负责成图。少填表，多看成稿。",
      continueSetup: "继续接入流程",
      openWorkspace: "打开工作台",
      refreshStatus: "刷新状态",
      loginWorkspace: "登录工作台",
      createAccount: "创建账号",
      meta: ["邮箱/手机号登录", "真实商品读取", "龙虾/Codex 出图"],
      emailOnlyMeta: ["邮箱登录", "真实商品读取", "龙虾/Codex 出图"],
      visualAria: "商品海报生成预览",
      visual: {
        toolbarLabel: "Live product brief",
        toolbarItem: "Ozon item #3169219",
        sourceLabel: "商品原图",
        sourceTitle: "车系打火机",
        sourceBody: "紫色车贴 · 金属喷嘴 · 随身款",
        outputLabel: "海报成稿",
        outputTitle: "时尚车系打火机",
        outputBody: "点亮风格，随身有行",
        briefLabel: "生成 brief",
        briefItems: ["保留商品颜色和车系元素", "突出便携、防风和礼品感", "禁止改品牌、改外观、乱写参数"],
        marquee: ["新品首图", "节日促销", "车品风格", "黑金质感", "俄语卖点", "竖版社媒", "商品对比"]
      }
    },
    capabilities: {
      eyebrow: "能做什么",
      title: "先把商品拿准，再谈生成效果。",
      cards: [
        {
          title: "店铺商品是真实来源",
          text: "读取 Ozon 商品详情和图片，海报从真实资料开始，不让模型凭空编。"
        },
        {
          title: "电脑助手保管授权",
          text: "店铺密钥留在本机，网页只看连接状态，断在哪一步就显示哪一步。"
        },
        {
          title: "成稿要能复核",
          text: "生成后检查商品外观、卖点和文字，明显跑偏就不当成成功。"
        }
      ]
    },
    workflow: {
      eyebrow: "上手路径",
      title: "用户只需要顺着下一步走。",
      text: "默认路径保留给新手，排查信息收起来。客服需要定位问题时，再看版本、节点和授权状态。",
      steps: [
        { title: "登录账号", text: "使用页面展示的已开通入口；手机号验证码没接通时，不会放出可点击按钮。" },
        { title: "安装电脑助手", text: "电脑助手负责保存店铺授权信息，网页不会保存你的店铺密钥。" },
        { title: "连接这台电脑", text: "打开电脑助手后，网页会确认它是否已经在运行。" },
        { title: "打开工作台", text: "在工作台检查店铺授权，读取真实商品，并生成不乱写卖点的海报。" }
      ]
    },
    console: {
      identity: {
        eyebrow: "已登录",
        serviceStatus: "服务状态",
        serviceOpen: "已开通",
        servicePending: "申请处理中",
        serviceClosed: "未开通",
        helper: "电脑助手",
        helperConnected: "已连接",
        helperDisconnected: "未连接",
        computerAuth: "电脑授权",
        authComplete: "已完成",
        authIncomplete: "未完成",
        storeAuth: "店铺授权",
        saved: "已保存",
        pendingFill: "待填写",
        notStarted: "未开始"
      },
      localDev: {
        summary: "开发调试",
        tokenTitle: "Nebula access_token",
        tokenText: "仅用于开发诊断：用已登录的 Nebula 会话换取 Ozon 服务会话。",
        tokenPlaceholder: "从 Nebula 开发环境获取",
        createServiceSession: "创建服务会话",
        localAccountTitle: "local_dev 账户",
        localAccountText: "仅用于离线调试；正式用户必须通过 Nebula，身份来源应显示 Nebula。",
        modeAria: "本地账号模式",
        methodAria: "本地登录方式",
        register: "注册",
        login: "登录",
        email: "邮箱",
        phone: "手机号",
        nebula: "账号编号",
        name: "昵称",
        namePlaceholder: "Ozon operator",
        localPassword: "本地密码",
        localPasswordPlaceholder: "仅 local_dev 使用",
        createLocalDev: "创建 local_dev",
        loginLocalDev: "登录 local_dev"
      },
      setup: {
        nextStep: "下一步",
        stepsAria: "接入步骤",
        actions: {
          refreshOrder: "刷新开通状态",
          copyOrder: "复制申请信息",
          openService: "开通服务",
          openedProbe: "我已打开，检测一下",
          authorizeComputer: "授权这台电脑",
          completeComputerAuth: "完成电脑授权",
          openWorkspace: "打开工作台",
          continueOnLocal: "去电脑上的 Ozon Local 继续",
          packagePreparing: "安装包准备中",
          refreshAfterHandled: "我已处理，刷新状态",
          openLocalApp: "请打开电脑上的 Ozon Local"
        },
        step1: {
          title: "开通服务",
          done: "服务已开通，可以继续连接电脑。",
          pending: "申请已经提交。付款或客服确认后，点刷新查看结果。",
          todo: "先开通服务，后面才能连接电脑和读取商品。"
        },
        step2: {
          title: "安装并打开电脑助手",
          done: "电脑助手已经打开。",
          todo: "下载安装到这台电脑，打开后回到这里点“检测一下”。",
          locked: "服务开通后，这里会给你下载入口。"
        },
        step3: {
          title: "授权这台电脑",
          done: "这台电脑已经可以使用你的服务。",
          todo: "只允许已授权的电脑读取商品，并把任务交给龙虾/Codex。"
        },
        step4: {
          readyTitle: "开始读取商品",
          connectTitle: "连接 Ozon 店铺",
          locked: "前面几步完成后，这里会告诉你去哪里填写店铺授权。",
          ready: "店铺授权已经保存。打开工作台读取商品，再把海报任务复制给龙虾/Codex。",
          todo: "切到电脑上的 Ozon Local，在“店铺授权”里填写 Ozon Client ID 和 API Key，保存后回这里刷新。"
        },
        handoff: {
          readyBadge: "店铺已连好",
          todoBadge: "现在去电脑助手",
          readyTitle: "可以读取真实商品了",
          todoTitle: "把 Ozon API 填到电脑助手里",
          readyText: "下一步在 Ozon Local 里读取商品，点“复制给龙虾/Codex”。图片 API 只是自动后台出图的可选项。",
          todoText: "网页已经确认这台电脑能用。接下来不是在网页里填密钥，而是在电脑助手里保存店铺授权，这样密钥只留在你的电脑上。",
          refreshCheck: "刷新检查"
        }
      },
      payment: {
        wechatTitle: "微信扫码支付",
        paymentReference: "支付备注",
        openCheckout: "打开支付页"
      },
      supportCode: {
        summaryTitle: "客服给了开通码？",
        summaryText: "有开通码时再打开这里。",
        code: "开通码",
        serviceStatus: "服务状态",
        expiry: "有效期",
        pendingExpiry: "开通后显示",
        redeem: "使用开通码"
      },
      orderDetails: {
        summaryTitle: "申请详情",
        summaryText: "需要发给客服或核对付款时再打开。",
        id: "申请编号",
        paymentReference: "支付备注",
        status: "状态",
        provider: "通道",
        amount: "金额",
        empty: "还没有申请记录。点“开通服务”后，这里会显示申请编号和付款备注。",
        copy: "复制申请信息",
        refresh: "刷新状态"
      },
      advanced: {
        summaryTitle: "排查用信息",
        summaryText: "一般不用看；客服排查安装或插件问题时再打开。",
        helper: "电脑助手",
        computerAuth: "电脑授权",
        storeAuth: "店铺授权",
        storeReady: "已保存",
        storePending: "待填写",
        storeReadyText: "电脑助手已保存 Ozon 店铺授权，可以读取商品。",
        storePendingText: "切到 Ozon Local，在店铺授权里填写 Client ID 和 API Key。",
        posterApi: "API 自动出图",
        posterReady: "已配置",
        posterOptional: "可选",
        posterReadyText: (model: string) => `图片 API 已保存：${model}。`,
        posterOptionalText: "默认用龙虾/Codex 出图；只有需要后台自动生成时才配置图片 API。",
        packageVersion: "安装包版本",
        releaseCheck: "发布校验",
        probe: "检测连接",
        pluginPackage: "插件安装包",
        copyPluginUrl: "复制插件连接地址",
        approvalRequired: "需要确认",
        readOnly: "只读"
      },
      device: {
        summaryTitle: "更换电脑或改名称",
        summaryText: "需要改设备名、查看授权时间时再打开。",
        name: "设备名",
        code: "设备码",
        codePlaceholder: "连接电脑助手后自动生成",
        codeTitle: "设备码由电脑助手生成，门户不允许手写伪造",
        authStatus: "授权状态",
        complete: "已完成",
        almostDone: "还差最后一步",
        unauthorized: "未授权",
        authorize: "授权这台电脑",
        completeAuth: "完成授权",
        lease: "授权",
        expiresAt: (value: string) => `有效期至 ${value}`
      }
    },
    pricing: {
      eyebrow: "开始接入",
      title: "进入工作台，按步骤完成 Ozon 商品读取。",
      text: "登录后页面会告诉你下一步该点什么：开通服务、安装电脑助手、连接这台电脑，然后开始读取商品。",
      accountAuth: "进入账户授权",
      start: "开始使用"
    },
    auth: {
      closeAria: "关闭登录面板",
      titleRegister: "创建账号",
      titleLogin: "登录工作台",
      descriptionRegister: "账号创建后，按页面提示开通服务、安装电脑助手并连接店铺。",
      descriptionLogin: "登录后继续完成开通、安装和电脑授权。",
      contextDirect: "默认使用邮箱或手机号登录；统一身份入口只作为企业账号备用。",
      contextDirectEmailOnly: "当前只开放邮箱登录；手机号验证码未接通时不会显示可点击入口。",
      contextSsoOnly: "当前只配置了企业统一身份入口；如果安全验证失败，请联系运营开通邮箱/手机号入口。",
      contextPasswordOrSso: "用邮箱 + 密码即可直接注册或登录；也可使用企业统一身份入口。",
      contextMaintenance: "账号服务正在维护，请联系运营支持。",
      directTitleRegister: "创建账号",
      directTitleLogin: "邮箱/手机号登录",
      directTitleRegisterEmailOnly: "邮箱创建账号",
      directTitleLoginEmailOnly: "邮箱登录",
      directText: "不跳出当前门户。登录后继续开通服务、安装电脑助手并读取商品。",
      directTextEmailOnly: "当前只开放邮箱登录。短信验证码接线完成前，页面不会出现手机号入口。",
      methodsAria: "账号登录方式",
      email: "邮箱",
      phone: "手机号",
      phoneDisabled: "手机号未开通",
      phoneAuthUnavailable: "手机号验证码尚未接通；当前只开放邮箱登录。",
      nebula: "账号编号",
      name: "昵称",
      namePlaceholder: "姓名或团队昵称",
      backupEmail: "备用邮箱",
      backupEmailPlaceholder: "用于接收账号通知",
      smsCode: "短信验证码",
      smsCodePlaceholder: "请输入验证码",
      password: "密码",
      passwordPlaceholder: "请输入密码",
      emailPlaceholder: "name@example.com",
      passwordTitle: "邮箱 + 密码",
      passwordText: "用邮箱和密码直接注册或登录，无需企业身份验证。",
      passwordRegister: "注册",
      passwordLogin: "登录",
      passwordSubmitRegister: "创建账号",
      passwordSubmitLogin: "登录",
      requestCode: "获取验证码",
      needsVerification: "当前登录需要先完成安全验证，验证通过后再提交。",
      ssoSummary: "企业统一身份入口",
      ssoTitle: (mode: "register" | "login") => `统一身份${mode === "register" ? "注册" : "登录"}`,
      ssoText: "仅在企业账号或客服要求时使用；打开后会离开当前页面完成验证。",
      openSsoRegister: "打开统一注册页",
      openSsoLogin: "打开统一登录页",
      captchaTitle: "这次登录还差安全验证",
      captchaText: "账号服务要求先完成验证码或二次确认。请刷新后重试；如果仍失败，请联系运营处理账号风控。",
      switchQuestionLogin: "还没有账号？",
      switchQuestionRegister: "已有账号？",
      switchToRegister: "创建账号",
      switchToLogin: "登录",
      submit: {
        emailRegister: "邮箱注册",
        emailLogin: "邮箱登录",
        phoneRegister: "手机号注册",
        phoneLogin: "手机号登录",
        nebulaLogin: "账号编号登录"
      }
    },
    messages: {
      turnstileWaiting: "等待安全验证",
      turnstileNotRequired: "无需验证",
      restoredSession: "已恢复本地会话，正在等待刷新",
      chooseLogin: "请选择已开通的登录方式",
      localNodeIdle: "登录后会检测电脑助手是否打开",
      directAuthUnavailable: "邮箱/手机号入口尚未开通，请联系运营支持。",
      sessionExpired: "登录已过期，请重新登录",
      openingUnified: (flow: "register" | "login") => `正在打开统一身份${flow === "register" ? "注册" : "登录"}页`,
      unifiedStartFailed: (error: string) => `统一授权启动失败：${error}`,
      completingUnifiedCallback: "正在完成统一身份授权回调",
      unifiedContextExpired: "授权上下文已失效，请从门户重新发起登录",
      unifiedAuthFailed: (error: string) => `统一身份授权失败：${error}`,
      unifiedCallbackMissing: "授权回调缺少 code/state，请重新登录",
      unifiedStateFailed: "授权状态校验失败，请重新登录",
      unifiedLoginFailed: (error: string) => `统一身份登录失败：${error}`,
      fillPhoneAndCode: "请填写手机号和短信验证码",
      fillMethodAndPassword: (method: string) => `请填写${method}和密码`,
      authenticatingMethod: (mode: "register" | "login", method: string) => `${mode === "register" ? "正在注册" : "正在验证"}${method}`,
      invalidPhone: "请填写有效手机号，例如 +8613800138000",
      phoneRegisterNeedsProfile: "手机号注册需要填写昵称和联系邮箱",
      accountLoginFailed: (error: string) => `账号登录失败：${error}`,
      fillPhoneFirst: "请先填写手机号",
      phoneRegisterNeedsProfileBeforeCode: "手机号注册需要先填写昵称和联系邮箱",
      completeSecurityBeforeCode: "请先完成安全验证，再获取短信验证码",
      requestingSmsCode: "正在请求短信验证码",
      smsCodeSent: "短信验证码已发送，请查收后继续",
      smsCodeSendFailedFallback: "短信验证码发送失败",
      smsCodeFailed: (error: string) => `短信验证码发送失败：${error}`,
      pasteSessionToken: "请粘贴已登录会话 token",
      creatingFromIdentityToken: "正在用身份会话创建 Ozon 服务会话",
      identityExchangeFailed: (error: string) => `身份会话交换失败：${error}`,
      creatingServiceSession: "正在创建 Ozon 服务会话",
      accountLoggedIn: (alias: string) => `账号已登录：${alias}`,
      authenticatingLocalDev: "正在使用本地开发入口",
      localDevSessionCreated: (nebulaId: string) => `local_dev 会话已建立：${nebulaId}`,
      localDevFailed: (error: string) => `本地开发入口失败：${error}`,
      loginRequired: "请先登录账号",
      refreshingAccount: "正在刷新账户状态",
      accountRefreshed: "账户状态已刷新",
      accountRefreshFailed: (error: string) => `账户刷新失败：${error}`,
      signedOut: "已退出登录",
      orderCreatedOpeningPayment: "订单已创建，正在打开支付页",
      orderCreatedManual: "订单已创建，请按支付备注完成确认",
      createOrderFailed: (error: string) => `创建订单失败：${error}`,
      noOrderToCopy: "还没有可复制的订单",
      orderInfoCopied: "申请信息已复制",
      copyFailed: "复制失败，请手动复制",
      noOrderToRefresh: "还没有可刷新的订单",
      orderStatusRefreshed: (status: string) => `开通状态已刷新：${status}`,
      refreshOrderFailed: (error: string) => `刷新订单失败：${error}`,
      redeemNeedsLoginAndCode: "需要登录并填写开通码",
      cardRedeemed: "开通码已使用，服务已开通",
      redeemFailed: (error: string) => `开通失败：${error}`,
      deviceActivated: "这台电脑已加入你的账号",
      deviceActivateFailed: (error: string) => `授权这台电脑失败：${error}`,
      needsLoginAndDevice: "需要先登录并绑定设备",
      computerAuthorized: "这台电脑已完成授权",
      computerAuthorizeFailed: (error: string) => `电脑授权失败：${error}`,
      checkingLocalNode: "正在检测电脑助手是否已打开",
      copyManifestAfterConnect: "电脑助手连接成功后，才能复制 OpenClaw 连接地址",
      manifestCopied: "OpenClaw 连接地址已复制",
      restoringAccount: "正在恢复账户状态",
      checkoutSuccess: "支付完成，正在刷新授权状态",
      checkoutCancelled: "支付已取消，订单还没有扣款",
      loadingTurnstile: "正在加载安全验证",
      turnstilePassed: "安全验证已通过",
      turnstileExpired: "安全验证已过期，请重新验证",
      turnstileRenderFailed: (error: string) => `安全验证组件渲染失败：${error}`,
      turnstileLoaded: "安全验证组件已加载；完成验证后可提交",
      turnstileHiddenHint: "如果没有看到安全验证，请刷新页面或联系运营支持",
      turnstileLoadFailed: (error: string) => `安全验证组件加载失败：${error}`,
      requestTimeout: "请求超时，请检查账号服务或 Ozon 服务是否可访问",
      realOzonEnabled: "真实 Ozon 商品读取已开启",
      devMode: "开发模式",
      localNodeOnlineWithIssue: (version: string, mode: string, issue: string) => `电脑助手${version}已打开，${mode}；${issue}`,
      localNodeOnline: (version: string, mode: string) => `电脑助手${version}已连接，${mode}`,
      localNodeHealthBlocked: "网页没有找到电脑助手。请先打开电脑助手；如果已经打开，安装最新版后再试。",
      localNodeNoResponse: "电脑助手没有回应。请确认已打开，或重启电脑助手后再点检测。",
      localNodeManifestFailed: "电脑助手已打开，但连接信息读取失败。请重启电脑助手，仍不行就安装最新版。",
      localNodeBlocked: "网页没有找到电脑助手。请打开电脑助手；如果还是不行，安装最新版后再点检测。",
      localNodeNotDetected: "未检测到电脑助手。请确认已打开，或安装最新版后再点检测。",
      confirmedManualOrder: "申请已人工确认。请使用运营发送的开通码完成开通。",
      confirmedOrder: "开通已确认，刷新账户后即可继续下一步",
      localLeaseBrowserBlock: "网页没有连上电脑助手。请先打开电脑助手，再点“我已打开，检测一下”。",
      localLeaseOldHelper: "你现在打开的是旧版电脑助手。请下载安装最新版，打开后再点“完成授权”。",
      localLeaseExpired: "电脑授权已过期，请重新完成授权。",
      localLeaseInvalid: "电脑里的授权记录已失效，请重新点“完成授权”。",
      localLeaseMissing: "电脑助手没有保存授权。请重启电脑助手，仍不行就安装最新版后再试。",
      localNodeIssue: "账户授权状态读取失败。请重启电脑助手，仍不行就安装最新版。",
      cloudApiRequired: "VITE_CLOUD_API must be configured for this production portal host",
      directAuthNotConfigured: "直接账号登录未配置",
      nebulaCannotRegister: "账号编号由系统分配，注册请使用邮箱或手机号",
      authFailed: "账号认证失败",
      emailVerifyThenLogin: "身份服务已接受请求，请完成邮箱验证后再登录",
      phoneOtpFailed: "手机号验证码登录失败",
      phoneOtpInvalid: "手机号验证码响应无效",
      phoneProfileWriteFailed: "手机号注册资料回写失败",
      userProfileFailed: "用户资料读取失败",
      nebulaLoginFailed: "账号编号登录失败",
      nebulaLoginInvalid: "账号编号登录响应无效",
      oauthConfigRequired: "请先配置 VITE_NEBULA_BASE_URL 和 VITE_NEBULA_CLIENT_ID",
      cryptoUnsupported: "当前浏览器不支持 Web Crypto，无法启动 PKCE 授权",
      webLoginFailed: "网页登录失败",
      webLoginInvalid: "网页登录返回信息无效",
      turnstileScriptNotConfigured: "安全验证脚本未配置",
      turnstileScriptLoadFailed: "Cloudflare Turnstile 脚本加载失败",
      turnstileServiceBlocked: "安全验证没有加载成功。请确认浏览器没有拦截验证服务。",
      turnstileNotPassed: "安全验证没有通过。请关闭代理或脚本拦截，换浏览器/网络后重试。",
      turnstileFailed: (code: string) => (code ? `安全验证失败：${code}` : "安全验证失败，请刷新页面后重试"),
      captchaProtection: "这次登录需要先完成安全验证。请刷新页面重试；如果仍失败，请联系运营处理账号风控。",
      missingRoot: "missing root element"
    },
    labels: {
      orderStatus: {
        confirmed: "已开通",
        pendingManualPayment: "等待付款确认",
        pending: "处理中",
        cancelled: "已取消"
      },
      paymentProvider: {
        manual: "人工确认",
        wechat: "微信支付",
        stripe: "Stripe"
      },
      localNodePhase: {
        checking: "检测中",
        online: "已在线",
        degraded: "部分可用",
        blocked: "未连上",
        offline: "未启动",
        idle: "待检测"
      },
      pairing: {
        serviceClosedTitle: "服务未开通",
        serviceClosedText: "先开通服务或输入开通码，才能连接店铺并读取商品。",
        connectHelperTitle: "先连接电脑助手",
        connectHelperText: "服务已开通。打开电脑助手并检测成功后，就能授权这台电脑。",
        notAuthorizedTitle: "还没授权这台电脑",
        notAuthorizedText: "点“授权这台电脑”，把当前电脑加入你的账号。",
        helperLeaseMissingTitle: "电脑助手还没保存授权",
        almostDoneTitle: "还差最后一步",
        confirmAgainText: "再点一次“完成授权”，电脑助手就能开始工作。",
        authorizedTitle: "这台电脑已授权",
        savedTitle: "电脑授权已保存",
        expiresAt: (value: string) => `有效期至 ${value}。`,
        beforeExpiry: "授权到期前"
      },
      setupStatus: {
        waitServiceTitle: "等待服务开通",
        openServiceTitle: "先开通服务",
        orderPendingText: "申请已经提交。付款或客服确认后，点“刷新开通状态”。",
        openServiceText: "开通后按提示安装电脑助手，再授权这台电脑。",
        installHelperTitle: "安装并打开电脑助手",
        installHelperText: "下载电脑助手，打开后回到这里点“检测一下”。",
        fillStoreTitle: "去电脑助手填写店铺 API",
        fillStoreText: "这台电脑已经授权。现在切到 Ozon Local，在店铺授权里保存 Ozon Client ID 和 API Key。",
        readyTitle: "可以开始了",
        readyTextBrief: "服务、电脑和店铺授权都准备好了。打开工作台读取商品，再把海报任务交给龙虾/Codex。",
        authorizeTitle: "授权这台电脑",
        authorizeText: "服务已开通，电脑助手也在线。现在把这台电脑加入账号。",
        incompleteTitle: "电脑授权没有完成",
        completeTitle: "完成电脑授权",
        completeText: "再确认一次授权，电脑助手就可以读取商品并交给龙虾/Codex。",
        readyTextImage: "服务、电脑和店铺授权都准备好了。打开工作台读取商品，再交给龙虾/Codex 出图。"
      },
      downloads: {
        macLabel: "Mac 安装包",
        macShort: "下载 Mac 版",
        windowsMsiLabel: "Windows 安装包",
        windowsMsiShort: "下载 Windows 版",
        windowsExeLabel: "Windows 备用安装包",
        windowsExeShort: "备用下载"
      },
      methods: {
        email: "邮箱",
        phone: "手机号",
        nebula: "账号编号"
      },
      placeholders: {
        phone: "请输入手机号",
        nebula: "请输入账号编号",
        email: "name@example.com"
      },
      display: {
        unbound: "未绑定",
        refreshing: "刷新中"
      }
    }
  },
  guide: {
    returnToPortal: "返回门户继续操作",
    hero: {
      eyebrow: "客户操作说明",
      title: "从登录到读取商品，再到生成海报",
      text: "按下面步骤操作即可。第一次使用需要安装电脑助手、授权当前电脑，并在电脑助手里填写 Ozon 店铺接口信息；以后只要打开网站和电脑助手，就可以继续读取商品。"
    },
    sideAria: "页面导航",
    quickJump: "快速跳转",
    navItems: [
      { href: "#prepare", label: "开始前准备" },
      { href: "#setup", label: "首次使用步骤" },
      { href: "#work", label: "读取商品和生成海报" },
      { href: "#daily", label: "日常使用" },
      { href: "#faq", label: "常见问题" },
      { href: "#support", label: "联系客服时提供什么" }
    ],
    prepare: {
      title: "开始前准备",
      items: [
        {
          label: "1. 一台常用电脑",
          text: "建议使用固定办公电脑。电脑助手安装在这台电脑后，商品读取和海报生成会通过这台电脑完成。"
        },
        {
          label: "2. Ozon Seller API 信息",
          text: "需要准备 Ozon Seller 后台里的 Client ID 和 API Key。它们用于读取你的店铺商品，不要发给无关人员。"
        },
        {
          label: "3. 一个可登录的 Ozon Rust Suite 账号",
          text: "使用页面已开通的登录方式。手机号验证码未接通时，网站不会显示可点击的发送验证码入口。"
        }
      ]
    },
    setup: {
      title: "首次使用步骤",
      steps: [
        {
          title: "打开网站并登录",
          paragraphs: ["访问 ozon66.com，点击登录。按页面提示使用已开通的登录方式；如果没有手机号入口，说明短信验证码尚未接通。出现安全验证时，请先完成验证。"],
          action: "打开登录页"
        },
        {
          title: "确认服务已开通",
          paragraphs: ["登录后查看页面左侧或顶部状态。如果显示“已开通”，可以继续下一步；如果显示“待确认”或“未开通”，请按页面提示提交开通申请或联系你的服务人员。"]
        },
        {
          title: "下载安装电脑助手",
          paragraphs: ["在“安装并打开电脑助手”步骤里，按你的电脑系统下载。"],
          items: ["Windows 用户：点击页面推荐的 Windows 安装包；如果安装受限，再尝试备用安装包。", "Mac 用户：按页面提供的 Mac 安装包操作；无法打开时联系服务人员确认电脑型号。"]
        },
        {
          title: "打开电脑助手",
          paragraphs: ["安装完成后打开 Ozon Local。第一次打开时，系统可能会询问是否允许网络访问，请选择允许。打开后回到网站，点击“检测电脑助手”。"]
        },
        {
          title: "授权这台电脑",
          paragraphs: ["网站检测到电脑助手后，会出现“授权这台电脑”或“完成授权”。点击后等待页面显示“可以开始了”。这一步完成后，这台电脑就可以读取店铺商品。"]
        },
        {
          title: "填写店铺授权信息",
          paragraphs: ["切到 Ozon Local 电脑助手，在“本地密钥”或“店铺授权”里填写 Ozon Seller 的 Client ID 和 API Key，点击保存。保存后回到网站，点击刷新或重新检测。"]
        }
      ]
    },
    work: {
      title: "读取商品和生成海报",
      steps: [
        {
          title: "进入工作台",
          paragraphs: ["网站显示“可以开始了”后，点击进入工作台。工作台会显示你的登录账号、电脑连接状态和店铺读取状态。"]
        },
        {
          title: "读取商品列表",
          paragraphs: ["点击“读取商品”。如果店铺授权正确，页面会显示商品数量和商品列表。商品较多时，第一次读取可能需要等几秒。", "如果列表里没有目标商品，可以输入 offer ID、商品 ID 或 sku 再读取详情。看到“归档商品”时，请先确认商品仍在销售，再用于海报或投放。"]
        },
        {
          title: "查看商品详情和图片",
          paragraphs: ["选择一个商品点击“详情/图片”，或输入商品的 offer ID 后点击“读取详情/图片”。页面会展示商品名称、商品图和可用于海报的基础信息。"]
        },
        {
          title: "生成海报简报",
          paragraphs: ["点击“生成海报简报”。系统会根据真实商品信息整理标题、卖点、图片参考和生成要求。生成前请确认商品图片和商品本身一致。"]
        },
        {
          title: "选择图片生成方式",
          paragraphs: ["建议优先使用已登录的龙虾、OpenClaw 或 Codex 继续生成图片。点击“复制给龙虾/Codex”后，切到已登录的工具里粘贴任务包，再按提示生成。", "如果你的账号已经开通后台图片生成服务，也可以选择后台自动生成。若页面提示“图片通道未开通”，请改用龙虾、OpenClaw 或 Codex 方式生成。"]
        },
        {
          title: "检查成图",
          paragraphs: ["海报生成后，请检查四件事：商品外观有没有变，包装文字有没有错，颜色和比例是否正常，卖点有没有夸大。如果不一致，重新生成或改简报后再生成。"]
        }
      ]
    },
    daily: {
      title: "日常使用",
      intro: "正常情况下，只需要三步：",
      items: ["打开 Ozon Local 电脑助手。", "打开 ozon66.com 并登录。", "进入工作台，点击读取商品，选择商品生成海报简报。"],
      outro: "如果网站提示电脑未连接，先确认电脑助手已经打开，再点击“检测电脑助手”。"
    },
    faq: {
      title: "常见问题",
      items: [
        {
          title: "登录后看不到下一步怎么办？",
          text: "先点击页面右上角“刷新”。如果仍然没有变化，可能是服务还没有完成开通确认，请联系服务人员。"
        },
        {
          title: "网站一直提示电脑助手未连接怎么办？",
          text: "确认 Ozon Local 已经打开。Windows 用户检查右下角托盘或开始菜单；Mac 用户检查“应用程序”里是否已经打开。如果刚安装完成，建议关闭浏览器页面后重新打开。"
        },
        {
          title: "保存 Ozon 信息后，仍然读不到商品怎么办？",
          text: "请确认 Client ID 和 API Key 来自正确的 Ozon Seller 店铺，并且没有多复制空格。保存后回网站点击刷新，再试一次读取商品。"
        },
        {
          title: "能读取商品，但自动生成图片失败怎么办？",
          text: "通常是图片生成通道没有开通。你仍然可以先使用“生成海报简报”，把简报复制到龙虾、OpenClaw 或 Codex 里生成图片。"
        },
        {
          title: "商品图片和成图不一致怎么办？",
          text: "不要直接使用。重新生成时明确要求保留商品包装、颜色、文字和比例。如果仍然不一致，换一张更清晰的商品主图。"
        }
      ]
    },
    support: {
      title: "联系客服时提供什么",
      intro: "遇到问题时，请把下面信息发给客服，能更快定位：",
      items: ["登录账号，例如邮箱或手机号。", "电脑系统：Windows 或 Mac。", "卡在哪一步：登录、安装电脑助手、授权电脑、保存店铺信息、读取商品、生成海报。", "页面上的提示文字或截图。", "如果是商品问题，请提供 offer ID 或商品链接。"],
      outro: "不要把完整的 Ozon API Key、登录密码或验证码发到公开群里。客服需要排查时，会告诉你具体发哪些信息。"
    },
    footer: "Ozon Rust Suite 使用说明。页面内容会随产品更新调整，请以 ozon66.com 当前页面提示为准。"
  }
};

type Messages = typeof zh;

const en: Messages = {
  common: {
    brand: "Ozon Rust Suite",
    details: {
      collapse: "Collapse",
      expand: "Expand"
    },
    language: {
      ariaLabel: "Switch language",
      zh: "中文",
      en: "English"
    }
  },
  portal: {
    nav: {
      primaryAria: "Primary navigation",
      loggedIn: {
        guide: "Setup",
        manual: "Guide",
        troubleshoot: "Troubleshooting"
      },
      public: {
        capabilities: "Features",
        workflow: "Workflow",
        pricing: "Plan",
        manual: "Guide"
      },
      actions: {
        refresh: "Refresh",
        logout: "Sign out",
        login: "Log in",
        register: "Create account"
      }
    },
    defaults: {
      deviceName: "My computer",
      releasePending: "Pending sync",
      releaseChecksumPending: "Waiting for release-manifest.json",
      downloadSyncPending: "Waiting for download information",
      currentModel: "current model"
    },
    hero: {
      eyebrow: "Ozon Rust Suite",
      titleLine1: "Product images in, ",
      titleLine2: "ready posters out.",
      description: "Read real product images, titles, and selling points. The local helper keeps store credentials on this computer, while OpenClaw/Codex handles image generation.",
      continueSetup: "Continue setup",
      openWorkspace: "Open workspace",
      refreshStatus: "Refresh status",
      loginWorkspace: "Log in",
      createAccount: "Create account",
      meta: ["Email or phone login", "Real product data", "OpenClaw/Codex images"],
      emailOnlyMeta: ["Email login", "Real product data", "OpenClaw/Codex images"],
      visualAria: "Product poster generation preview",
      visual: {
        toolbarLabel: "Live product brief",
        toolbarItem: "Ozon item #3169219",
        sourceLabel: "Source product",
        sourceTitle: "Car-series lighter",
        sourceBody: "Purple car decal · metal nozzle · pocket size",
        outputLabel: "Poster output",
        outputTitle: "Stylish car-series lighter",
        outputBody: "Light up your style on the go",
        briefLabel: "Generation brief",
        briefItems: ["Keep the product colors and car styling", "Emphasize portability, wind resistance, and gifting", "Do not change the brand, shape, or specs"],
        marquee: ["New arrival", "Holiday promo", "Auto style", "Black-gold look", "Russian copy", "Vertical social", "Product compare"]
      }
    },
    capabilities: {
      eyebrow: "What it does",
      title: "Start with accurate product data, then judge generation quality.",
      cards: [
        {
          title: "Store products are the source of truth",
          text: "Read Ozon product details and images so posters begin from real listing data instead of invented model context."
        },
        {
          title: "The helper keeps store credentials local",
          text: "Store keys stay on this computer. The portal only reads connection status and shows where setup stopped."
        },
        {
          title: "Outputs remain reviewable",
          text: "After generation, check the appearance, claims, and text. Obvious drift should not be treated as success."
        }
      ]
    },
    workflow: {
      eyebrow: "Getting started",
      title: "Users follow the next step on screen.",
      text: "The default path stays simple for new users, while diagnostic details stay collapsed until support needs versions, endpoints, or authorization state.",
      steps: [
        { title: "Log in", text: "Use the enabled entry shown on the page. If phone SMS is not wired, no clickable code button is shown." },
        { title: "Install the helper", text: "The helper stores store authorization locally; the web portal never stores your store keys." },
        { title: "Connect this computer", text: "After the helper is open, the portal verifies that it is running." },
        { title: "Open the workspace", text: "Check store authorization, read real products, and generate posters without invented selling points." }
      ]
    },
    console: {
      identity: {
        eyebrow: "Signed in",
        serviceStatus: "Service",
        serviceOpen: "Active",
        servicePending: "Pending",
        serviceClosed: "Inactive",
        helper: "Helper",
        helperConnected: "Connected",
        helperDisconnected: "Disconnected",
        computerAuth: "Computer auth",
        authComplete: "Complete",
        authIncomplete: "Incomplete",
        storeAuth: "Store auth",
        saved: "Saved",
        pendingFill: "Needs setup",
        notStarted: "Not started"
      },
      localDev: {
        summary: "Development diagnostics",
        tokenTitle: "Nebula access_token",
        tokenText: "Development diagnostics only: exchange a signed-in Nebula session for an Ozon service session.",
        tokenPlaceholder: "Get it from the Nebula development environment",
        createServiceSession: "Create service session",
        localAccountTitle: "local_dev account",
        localAccountText: "Offline debugging only. Production users must use Nebula, and identity source should show Nebula.",
        modeAria: "Local account mode",
        methodAria: "Local login method",
        register: "Register",
        login: "Log in",
        email: "Email",
        phone: "Phone",
        nebula: "Account ID",
        name: "Name",
        namePlaceholder: "Ozon operator",
        localPassword: "Local password",
        localPasswordPlaceholder: "Only for local_dev",
        createLocalDev: "Create local_dev",
        loginLocalDev: "Log in local_dev"
      },
      setup: {
        nextStep: "Next step",
        stepsAria: "Setup steps",
        actions: {
          refreshOrder: "Refresh activation",
          copyOrder: "Copy request info",
          openService: "Activate service",
          openedProbe: "I opened it, check",
          authorizeComputer: "Authorize this computer",
          completeComputerAuth: "Finish computer authorization",
          openWorkspace: "Open workspace",
          continueOnLocal: "Continue in Ozon Local",
          packagePreparing: "Installer preparing",
          refreshAfterHandled: "Done, refresh status",
          openLocalApp: "Open Ozon Local on this computer"
        },
        step1: {
          title: "Activate service",
          done: "Service is active. You can connect the computer next.",
          pending: "The request was submitted. Pay or wait for support confirmation, then refresh.",
          todo: "Activate the service before connecting the computer and reading products."
        },
        step2: {
          title: "Install and open the helper",
          done: "The helper is open.",
          todo: "Install it on this computer, open it, then come back and click Check.",
          locked: "After service activation, the download entry appears here."
        },
        step3: {
          title: "Authorize this computer",
          done: "This computer can now use your service.",
          todo: "Only authorized computers may read products and hand tasks to OpenClaw/Codex."
        },
        step4: {
          readyTitle: "Start reading products",
          connectTitle: "Connect Ozon store",
          locked: "After the previous steps, this area will show where to add store authorization.",
          ready: "Store authorization is saved. Open the workspace, read products, then copy poster tasks to OpenClaw/Codex.",
          todo: "Open Ozon Local on this computer, add Ozon Client ID and API Key under Store Authorization, save, then refresh here."
        },
        handoff: {
          readyBadge: "Store connected",
          todoBadge: "Go to the helper",
          readyTitle: "Real product reading is ready",
          todoTitle: "Add the Ozon API keys in the helper",
          readyText: "Next, read products in Ozon Local and click Copy for OpenClaw/Codex. The image API is optional for automated background generation.",
          todoText: "The portal has confirmed this computer can be used. Store keys are not entered in the web page; save them in the helper so they remain on your computer.",
          refreshCheck: "Refresh check"
        }
      },
      payment: {
        wechatTitle: "WeChat scan payment",
        paymentReference: "Payment note",
        openCheckout: "Open checkout"
      },
      supportCode: {
        summaryTitle: "Did support give you an activation code?",
        summaryText: "Open this only when you have a code.",
        code: "Activation code",
        serviceStatus: "Service status",
        expiry: "Expires",
        pendingExpiry: "Shown after activation",
        redeem: "Use activation code"
      },
      orderDetails: {
        summaryTitle: "Request details",
        summaryText: "Open this when support asks for it or you need to check payment.",
        id: "Request ID",
        paymentReference: "Payment note",
        status: "Status",
        provider: "Channel",
        amount: "Amount",
        empty: "No request yet. After clicking Activate service, the request ID and payment note will appear here.",
        copy: "Copy request info",
        refresh: "Refresh status"
      },
      advanced: {
        summaryTitle: "Diagnostics",
        summaryText: "Usually hidden; open it when support is troubleshooting installation or plugin issues.",
        helper: "Helper",
        computerAuth: "Computer auth",
        storeAuth: "Store auth",
        storeReady: "Saved",
        storePending: "Needs setup",
        storeReadyText: "The helper has saved Ozon store authorization and can read products.",
        storePendingText: "Open Ozon Local and enter Client ID and API Key under Store Authorization.",
        posterApi: "Image API",
        posterReady: "Configured",
        posterOptional: "Optional",
        posterReadyText: (model: string) => `Image API saved: ${model}.`,
        posterOptionalText: "By default, use OpenClaw/Codex for images. Configure the image API only for automated background generation.",
        packageVersion: "Installer version",
        releaseCheck: "Release check",
        probe: "Check connection",
        pluginPackage: "Plugin package",
        copyPluginUrl: "Copy plugin connection URL",
        approvalRequired: "Approval required",
        readOnly: "Read-only"
      },
      device: {
        summaryTitle: "Change computer or name",
        summaryText: "Open this to rename the device or inspect authorization time.",
        name: "Device name",
        code: "Device code",
        codePlaceholder: "Generated after connecting the helper",
        codeTitle: "The device code is generated by the helper and cannot be forged in the portal.",
        authStatus: "Authorization status",
        complete: "Complete",
        almostDone: "One more step",
        unauthorized: "Unauthorized",
        authorize: "Authorize this computer",
        completeAuth: "Finish authorization",
        lease: "Authorization",
        expiresAt: (value: string) => `Expires at ${value}`
      }
    },
    pricing: {
      eyebrow: "Start setup",
      title: "Enter the workspace and complete Ozon product reading step by step.",
      text: "After login, the page tells you what to do next: activate the service, install the helper, connect this computer, then read products.",
      accountAuth: "Go to account authorization",
      start: "Get started"
    },
    auth: {
      closeAria: "Close login panel",
      titleRegister: "Create account",
      titleLogin: "Log in",
      descriptionRegister: "After creating the account, follow the page to activate service, install the helper, and connect the store.",
      descriptionLogin: "Log in to continue activation, installation, and computer authorization.",
      contextDirect: "Email or phone is the default login method; enterprise identity is a backup for company accounts.",
      contextDirectEmailOnly: "Email login is currently enabled. Phone SMS is not shown as clickable until it is wired.",
      contextSsoOnly: "Only enterprise identity is configured. If security verification fails, contact support to enable email or phone login.",
      contextPasswordOrSso: "Register or sign in directly with email + password; enterprise identity is also available.",
      contextMaintenance: "Account service is under maintenance. Contact support.",
      directTitleRegister: "Create account",
      directTitleLogin: "Email or phone login",
      directTitleRegisterEmailOnly: "Create account with email",
      directTitleLoginEmailOnly: "Email login",
      directText: "Stay in this portal. After login, continue activation, helper installation, and product reading.",
      directTextEmailOnly: "Email login is active. The phone entry is hidden until SMS verification is wired.",
      methodsAria: "Account login method",
      email: "Email",
      phone: "Phone",
      phoneDisabled: "Phone unavailable",
      phoneAuthUnavailable: "Phone SMS verification is not wired yet. Use email login for now.",
      nebula: "Account ID",
      name: "Name",
      namePlaceholder: "Name or team nickname",
      backupEmail: "Backup email",
      backupEmailPlaceholder: "Used for account notifications",
      smsCode: "SMS code",
      smsCodePlaceholder: "Enter code",
      password: "Password",
      passwordPlaceholder: "Enter password",
      emailPlaceholder: "name@example.com",
      passwordTitle: "Email + password",
      passwordText: "Register or sign in directly with your email and password — no enterprise identity needed.",
      passwordRegister: "Register",
      passwordLogin: "Log in",
      passwordSubmitRegister: "Create account",
      passwordSubmitLogin: "Log in",
      requestCode: "Get code",
      needsVerification: "Complete security verification before submitting.",
      ssoSummary: "Enterprise identity entry",
      ssoTitle: (mode: "register" | "login") => `Enterprise identity ${mode === "register" ? "registration" : "login"}`,
      ssoText: "Use only for enterprise accounts or when support asks you to. It opens another page to complete verification.",
      openSsoRegister: "Open enterprise registration",
      openSsoLogin: "Open enterprise login",
      captchaTitle: "Security verification is still required",
      captchaText: "The account service requires a captcha or second confirmation. Refresh and try again; contact support if it still fails.",
      switchQuestionLogin: "No account yet?",
      switchQuestionRegister: "Already have an account?",
      switchToRegister: "Create account",
      switchToLogin: "Log in",
      submit: {
        emailRegister: "Register with email",
        emailLogin: "Log in with email",
        phoneRegister: "Register with phone",
        phoneLogin: "Log in with phone",
        nebulaLogin: "Log in with account ID"
      }
    },
    messages: {
      turnstileWaiting: "Waiting for security verification",
      turnstileNotRequired: "No verification required",
      restoredSession: "Restored local session; waiting to refresh",
      chooseLogin: "Choose an enabled login method",
      localNodeIdle: "After login, the portal will check whether the helper is open",
      directAuthUnavailable: "Email/phone entry is not enabled yet. Contact operations support.",
      sessionExpired: "Session expired. Log in again.",
      openingUnified: (flow: "register" | "login") => `Opening enterprise identity ${flow === "register" ? "registration" : "login"} page`,
      unifiedStartFailed: (error: string) => `Failed to start enterprise authorization: ${error}`,
      completingUnifiedCallback: "Completing enterprise identity callback",
      unifiedContextExpired: "Authorization context expired. Start login again from the portal.",
      unifiedAuthFailed: (error: string) => `Enterprise authorization failed: ${error}`,
      unifiedCallbackMissing: "Authorization callback is missing code/state. Log in again.",
      unifiedStateFailed: "Authorization state check failed. Log in again.",
      unifiedLoginFailed: (error: string) => `Enterprise login failed: ${error}`,
      fillPhoneAndCode: "Enter phone number and SMS code",
      fillMethodAndPassword: (method: string) => `Enter ${method} and password`,
      authenticatingMethod: (mode: "register" | "login", method: string) => `${mode === "register" ? "Registering" : "Verifying"} ${method}`,
      invalidPhone: "Enter a valid phone number, for example +8613800138000",
      phoneRegisterNeedsProfile: "Phone registration requires a nickname and contact email",
      accountLoginFailed: (error: string) => `Account login failed: ${error}`,
      fillPhoneFirst: "Enter phone number first",
      phoneRegisterNeedsProfileBeforeCode: "Enter nickname and contact email before requesting the SMS code",
      completeSecurityBeforeCode: "Complete security verification before requesting the SMS code",
      requestingSmsCode: "Requesting SMS code",
      smsCodeSent: "SMS code sent. Check your phone and continue.",
      smsCodeSendFailedFallback: "Failed to send SMS code",
      smsCodeFailed: (error: string) => `Failed to send SMS code: ${error}`,
      pasteSessionToken: "Paste a signed-in session token",
      creatingFromIdentityToken: "Creating Ozon service session from identity session",
      identityExchangeFailed: (error: string) => `Identity session exchange failed: ${error}`,
      creatingServiceSession: "Creating Ozon service session",
      accountLoggedIn: (alias: string) => `Signed in: ${alias}`,
      authenticatingLocalDev: "Using local development entry",
      localDevSessionCreated: (nebulaId: string) => `local_dev session created: ${nebulaId}`,
      localDevFailed: (error: string) => `local_dev entry failed: ${error}`,
      loginRequired: "Log in first",
      refreshingAccount: "Refreshing account status",
      accountRefreshed: "Account status refreshed",
      accountRefreshFailed: (error: string) => `Account refresh failed: ${error}`,
      signedOut: "Signed out",
      orderCreatedOpeningPayment: "Request created. Opening payment page.",
      orderCreatedManual: "Request created. Complete confirmation with the payment note.",
      createOrderFailed: (error: string) => `Failed to create request: ${error}`,
      noOrderToCopy: "No request to copy yet",
      orderInfoCopied: "Request info copied",
      copyFailed: "Copy failed, please copy manually",
      noOrderToRefresh: "No request to refresh yet",
      orderStatusRefreshed: (status: string) => `Activation status refreshed: ${status}`,
      refreshOrderFailed: (error: string) => `Failed to refresh request: ${error}`,
      redeemNeedsLoginAndCode: "Log in and enter an activation code",
      cardRedeemed: "Activation code used. Service is active.",
      redeemFailed: (error: string) => `Activation failed: ${error}`,
      deviceActivated: "This computer has been added to your account",
      deviceActivateFailed: (error: string) => `Failed to authorize this computer: ${error}`,
      needsLoginAndDevice: "Log in and bind a device first",
      computerAuthorized: "This computer is authorized",
      computerAuthorizeFailed: (error: string) => `Computer authorization failed: ${error}`,
      checkingLocalNode: "Checking whether the helper is open",
      copyManifestAfterConnect: "Connect the helper before copying the OpenClaw connection URL",
      manifestCopied: "OpenClaw connection URL copied",
      restoringAccount: "Restoring account status",
      checkoutSuccess: "Payment completed. Refreshing authorization status.",
      checkoutCancelled: "Payment was cancelled. No charge was made.",
      loadingTurnstile: "Loading security verification",
      turnstilePassed: "Security verification passed",
      turnstileExpired: "Security verification expired. Verify again.",
      turnstileRenderFailed: (error: string) => `Security verification render failed: ${error}`,
      turnstileLoaded: "Security verification loaded. Submit after completing it.",
      turnstileHiddenHint: "If security verification is not visible, refresh the page or contact operations support",
      turnstileLoadFailed: (error: string) => `Failed to load security verification: ${error}`,
      requestTimeout: "Request timed out. Check whether account service or Ozon service is reachable.",
      realOzonEnabled: "Real Ozon product reading is enabled",
      devMode: "Development mode",
      localNodeOnlineWithIssue: (version: string, mode: string, issue: string) => `Helper${version} is open, ${mode}; ${issue}`,
      localNodeOnline: (version: string, mode: string) => `Helper${version} is connected, ${mode}`,
      localNodeHealthBlocked: "The page did not find the helper. Open it first; if it is already open, install the latest version and try again.",
      localNodeNoResponse: "The helper did not respond. Confirm it is open, or restart it and check again.",
      localNodeManifestFailed: "The helper is open, but connection info could not be read. Restart the helper or install the latest version.",
      localNodeBlocked: "The page did not find the helper. Open it; if it still fails, install the latest version and check again.",
      localNodeNotDetected: "Helper not detected. Confirm it is open, or install the latest version and check again.",
      confirmedManualOrder: "The request was manually confirmed. Use the activation code from operations to complete activation.",
      confirmedOrder: "Activation confirmed. Refresh account status to continue.",
      localLeaseBrowserBlock: "The page could not connect to the helper. Open the helper first, then click I opened it, check.",
      localLeaseOldHelper: "The helper version is too old. Install the latest version, open it, then click Finish authorization.",
      localLeaseExpired: "Computer authorization expired. Authorize again.",
      localLeaseInvalid: "The authorization record on this computer is invalid. Click Finish authorization again.",
      localLeaseMissing: "The helper did not save authorization. Restart it, or install the latest version and try again.",
      localNodeIssue: "Could not read account authorization status. Restart the helper, or install the latest version.",
      cloudApiRequired: "VITE_CLOUD_API must be configured for this production portal host",
      directAuthNotConfigured: "Direct account login is not configured",
      nebulaCannotRegister: "Account IDs are assigned by the system. Register with email or phone.",
      authFailed: "Account authentication failed",
      emailVerifyThenLogin: "Identity service accepted the request. Verify email before logging in.",
      phoneOtpFailed: "Phone verification login failed",
      phoneOtpInvalid: "Phone verification response is invalid",
      phoneProfileWriteFailed: "Failed to write phone registration profile",
      userProfileFailed: "Failed to read user profile",
      nebulaLoginFailed: "Account ID login failed",
      nebulaLoginInvalid: "Account ID login response is invalid",
      oauthConfigRequired: "Configure VITE_NEBULA_BASE_URL and VITE_NEBULA_CLIENT_ID first",
      cryptoUnsupported: "This browser does not support Web Crypto, so PKCE authorization cannot start",
      webLoginFailed: "Web login failed",
      webLoginInvalid: "Web login response is invalid",
      turnstileScriptNotConfigured: "Security verification script is not configured",
      turnstileScriptLoadFailed: "Cloudflare Turnstile script failed to load",
      turnstileServiceBlocked: "Security verification did not load. Check whether the browser blocked the verification service.",
      turnstileNotPassed: "Security verification did not pass. Disable proxy/script blocking or retry with another browser/network.",
      turnstileFailed: (code: string) => (code ? `Security verification failed: ${code}` : "Security verification failed. Refresh and try again."),
      captchaProtection: "Security verification is required for this login. Refresh and try again; contact operations if it still fails.",
      missingRoot: "missing root element"
    },
    labels: {
      orderStatus: {
        confirmed: "Active",
        pendingManualPayment: "Waiting for payment confirmation",
        pending: "Processing",
        cancelled: "Cancelled"
      },
      paymentProvider: {
        manual: "Manual confirmation",
        wechat: "WeChat Pay",
        stripe: "Stripe"
      },
      localNodePhase: {
        checking: "Checking",
        online: "Online",
        degraded: "Partially available",
        blocked: "Not connected",
        offline: "Not started",
        idle: "Not checked"
      },
      pairing: {
        serviceClosedTitle: "Service inactive",
        serviceClosedText: "Activate service or enter an activation code before connecting the store and reading products.",
        connectHelperTitle: "Connect the helper first",
        connectHelperText: "Service is active. Open the helper and check successfully before authorizing this computer.",
        notAuthorizedTitle: "This computer is not authorized yet",
        notAuthorizedText: "Click Authorize this computer to add the current computer to your account.",
        helperLeaseMissingTitle: "The helper has not saved authorization",
        almostDoneTitle: "One more step",
        confirmAgainText: "Click Finish authorization again so the helper can start working.",
        authorizedTitle: "This computer is authorized",
        savedTitle: "Computer authorization saved",
        expiresAt: (value: string) => `Expires at ${value}.`,
        beforeExpiry: "before authorization expires"
      },
      setupStatus: {
        waitServiceTitle: "Waiting for service activation",
        openServiceTitle: "Activate service first",
        orderPendingText: "The request has been submitted. After payment or support confirmation, click Refresh activation.",
        openServiceText: "After activation, install the helper and authorize this computer.",
        installHelperTitle: "Install and open the helper",
        installHelperText: "Download the helper, open it, then come back and click Check.",
        fillStoreTitle: "Add store API in the helper",
        fillStoreText: "This computer is authorized. Open Ozon Local and save Ozon Client ID and API Key under Store Authorization.",
        readyTitle: "Ready to start",
        readyTextBrief: "Service, computer, and store authorization are ready. Open the workspace, read products, then hand poster tasks to OpenClaw/Codex.",
        authorizeTitle: "Authorize this computer",
        authorizeText: "Service is active and the helper is online. Add this computer to the account now.",
        incompleteTitle: "Computer authorization is incomplete",
        completeTitle: "Finish computer authorization",
        completeText: "Confirm authorization once more so the helper can read products and hand tasks to OpenClaw/Codex.",
        readyTextImage: "Service, computer, and store authorization are ready. Open the workspace, read products, then generate images with OpenClaw/Codex."
      },
      downloads: {
        macLabel: "Mac installer",
        macShort: "Download for Mac",
        windowsMsiLabel: "Windows installer",
        windowsMsiShort: "Download for Windows",
        windowsExeLabel: "Windows backup installer",
        windowsExeShort: "Backup download"
      },
      methods: {
        email: "Email",
        phone: "Phone",
        nebula: "Account ID"
      },
      placeholders: {
        phone: "Enter phone number",
        nebula: "Enter account ID",
        email: "name@example.com"
      },
      display: {
        unbound: "Unbound",
        refreshing: "Refreshing"
      }
    }
  },
  guide: {
    returnToPortal: "Return to portal",
    hero: {
      eyebrow: "Customer guide",
      title: "From login to product reading to poster generation",
      text: "Follow the steps below. First-time setup requires installing the desktop helper, authorizing this computer, and entering Ozon store API information in the helper. After that, open the website and helper to keep reading products."
    },
    sideAria: "Page navigation",
    quickJump: "Quick jump",
    navItems: [
      { href: "#prepare", label: "Before you start" },
      { href: "#setup", label: "First-time setup" },
      { href: "#work", label: "Read products and generate posters" },
      { href: "#daily", label: "Daily use" },
      { href: "#faq", label: "FAQ" },
      { href: "#support", label: "What to send support" }
    ],
    prepare: {
      title: "Before you start",
      items: [
        {
          label: "1. A regular work computer",
          text: "Use a fixed office computer if possible. After the helper is installed there, product reading and poster generation will run through that computer."
        },
        {
          label: "2. Ozon Seller API information",
          text: "Prepare the Client ID and API Key from Ozon Seller. They are used to read your store products; do not send them to unrelated people."
        },
        {
          label: "3. An Ozon Rust Suite account you can log into",
          text: "Use the login method currently enabled on the page. If phone SMS is not wired, the site will not show a clickable code-sending entry."
        }
      ]
    },
    setup: {
      title: "First-time setup",
      steps: [
        {
          title: "Open the website and log in",
          paragraphs: ["Visit ozon66.com and click Log in. Use the enabled login method shown on the page. If there is no phone entry, SMS verification is not wired yet. Complete security verification first if it appears."],
          action: "Open login page"
        },
        {
          title: "Confirm service activation",
          paragraphs: ["After login, check the status on the left or top of the page. If it shows Active, continue. If it shows Pending or Inactive, submit an activation request or contact your service contact."]
        },
        {
          title: "Download and install the helper",
          paragraphs: ["In the Install and open the helper step, download the installer for your computer."],
          items: ["Windows users: click the recommended Windows installer. If installation is restricted, try the backup installer.", "Mac users: follow the Mac installer instructions. If it cannot open, contact support to confirm your Mac model."]
        },
        {
          title: "Open the helper",
          paragraphs: ["After installation, open Ozon Local. The system may ask whether to allow network access the first time; choose Allow. Return to the website and click Check helper."]
        },
        {
          title: "Authorize this computer",
          paragraphs: ["After the website detects the helper, Authorize this computer or Finish authorization will appear. Click it and wait until the page says Ready to start. This computer can then read store products."]
        },
        {
          title: "Enter store authorization",
          paragraphs: ["Switch to Ozon Local and enter Ozon Seller Client ID and API Key under Local keys or Store authorization, then save. Return to the website and refresh or check again."]
        }
      ]
    },
    work: {
      title: "Read products and generate posters",
      steps: [
        {
          title: "Enter the workspace",
          paragraphs: ["When the website says Ready to start, open the workspace. It shows your login account, computer connection status, and store reading status."]
        },
        {
          title: "Read product list",
          paragraphs: ["Click Read products. If store authorization is correct, the page shows product count and product list. Large stores may take a few seconds on the first read.", "If the target product is not listed, enter offer ID, product ID, or SKU and read details. If you see an archived product, confirm it is still on sale before using it for posters or campaigns."]
        },
        {
          title: "View product details and images",
          paragraphs: ["Select a product and click Details/images, or enter an offer ID and click Read details/images. The page shows product name, product images, and base information for the poster."]
        },
        {
          title: "Generate poster brief",
          paragraphs: ["Click Generate poster brief. The system organizes title, selling points, image references, and generation requirements from real product information. Confirm the product images match the product before generating."]
        },
        {
          title: "Choose image generation method",
          paragraphs: ["Prefer a signed-in OpenClaw or Codex session for image generation. Click Copy for OpenClaw/Codex, paste the task package into the signed-in tool, and generate as instructed.", "If your account has backend image generation enabled, you can also use automatic generation. If the page says image channel is not enabled, use OpenClaw or Codex instead."]
        },
        {
          title: "Check the result image",
          paragraphs: ["After poster generation, check four things: product appearance, package text, colors/proportions, and whether selling points are exaggerated. If anything is inconsistent, regenerate or revise the brief first."]
        }
      ]
    },
    daily: {
      title: "Daily use",
      intro: "Normally, only three steps are needed:",
      items: ["Open the Ozon Local helper.", "Open ozon66.com and log in.", "Enter the workspace, click Read products, choose a product, and generate a poster brief."],
      outro: "If the website says the computer is not connected, confirm the helper is open, then click Check helper."
    },
    faq: {
      title: "FAQ",
      items: [
        {
          title: "What if I do not see the next step after login?",
          text: "Click Refresh in the upper right first. If nothing changes, service activation may not be confirmed yet. Contact support."
        },
        {
          title: "What if the website keeps saying the helper is not connected?",
          text: "Confirm Ozon Local is open. Windows users should check the tray or Start menu; Mac users should check Applications. If you just installed it, close the browser page and open it again."
        },
        {
          title: "What if products still cannot be read after saving Ozon info?",
          text: "Confirm Client ID and API Key come from the correct Ozon Seller store and no extra spaces were copied. After saving, return to the website, refresh, and try reading products again."
        },
        {
          title: "What if products can be read but automatic image generation fails?",
          text: "Usually the image generation channel is not enabled. You can still generate the poster brief and copy it into OpenClaw or Codex for image generation."
        },
        {
          title: "What if the product image and generated image are inconsistent?",
          text: "Do not use it directly. When regenerating, explicitly require keeping the package, color, text, and proportions. If it still differs, use a clearer main product image."
        }
      ]
    },
    support: {
      title: "What to send support",
      intro: "When you encounter an issue, send the following information so support can locate it faster:",
      items: ["Login account, such as email or phone number.", "Computer system: Windows or Mac.", "Which step is blocked: login, installing helper, authorizing computer, saving store info, reading products, or generating posters.", "The prompt text on the page or a screenshot.", "For product issues, provide offer ID or product link."],
      outro: "Do not send the full Ozon API Key, login password, or verification code to public groups. Support will tell you exactly what information is needed for troubleshooting."
    },
    footer: "Ozon Rust Suite guide. Content may change as the product updates; follow the current prompts on ozon66.com."
  }
};

export const messages: Record<Locale, Messages> = { zh, en };

export function useI18n() {
  const [locale, setLocaleState] = useState<Locale>(() => resolveInitialLocale());

  useEffect(() => {
    applyDocumentLocale(locale);
  }, [locale]);

  const setLocale = useCallback((nextLocale: Locale) => {
    localStorage.setItem(LOCALE_STORAGE_KEY, nextLocale);
    applyDocumentLocale(nextLocale);
    setLocaleState(nextLocale);
  }, []);

  return useMemo(
    () => ({
      copy: messages[locale],
      locale,
      setLocale
    }),
    [locale, setLocale]
  );
}

export function getCurrentLocale(): Locale {
  return resolveInitialLocale();
}

export function getPortalCopy() {
  return messages[getCurrentLocale()].portal;
}

const statusTonePatterns = {
  danger: /(失败|不能|没有找到|没有保存|读取失败|未检测|太旧|不完整|失效|过期|failed|cannot|not found|not detected|invalid|expired|timed out|timeout|unavailable|blocked)/i,
  warn: /(请|需要|等待|未开通|未启动|处理中|please|need|waiting|pending|not started|inactive|processing|required)/i
};

const placeholderDisplayNames = ["用户", "新用户", "访客", "Apple 用户", "Nebula 用户", "User", "New User", "Guest"];

export function portalMessageTone(message: string): "ok" | "warn" | "danger" {
  if (statusTonePatterns.danger.test(message)) return "danger";
  if (statusTonePatterns.warn.test(message)) return "warn";
  return "ok";
}

export function isPlaceholderDisplayName(value: string | undefined) {
  const trimmed = value?.trim() ?? "";
  return !trimmed || placeholderDisplayNames.includes(trimmed);
}

function resolveInitialLocale(): Locale {
  const stored = safeStorageLocale();
  if (stored) return stored;
  const languages = navigator.languages?.length ? navigator.languages : [navigator.language];
  return languages.some((language) => language.toLowerCase().startsWith("zh")) ? "zh" : "en";
}

function safeStorageLocale(): Locale | null {
  try {
    const value = localStorage.getItem(LOCALE_STORAGE_KEY);
    return isLocale(value) ? value : null;
  } catch {
    return null;
  }
}

function isLocale(value: string | null): value is Locale {
  return value === "zh" || value === "en";
}

export function applyDocumentLocale(locale: Locale) {
  const copy = messages[locale];
  document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
  document.documentElement.dataset.locale = locale;
  document.documentElement.style.setProperty("--details-collapse-label", cssContent(copy.common.details.collapse));
  document.documentElement.style.setProperty("--details-expand-label", cssContent(copy.common.details.expand));
  document.documentElement.style.setProperty("--poster-art-title", cssContent(copy.portal.hero.visual.outputTitle));
}

function cssContent(value: string) {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}
