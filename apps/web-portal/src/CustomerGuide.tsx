import { Orbit } from "lucide-react";
import type { ReactNode } from "react";

export function CustomerGuide() {
  return (
    <main className="customer-guide">
      <header className="guide-topbar">
        <a className="brand-mark" href="/" aria-label="Ozon Rust Suite">
          <span className="brand-icon">
            <Orbit size={18} />
          </span>
          <span>Ozon Rust Suite</span>
        </a>
        <a className="guide-return" href="/">
          返回登录页
        </a>
      </header>

      <section className="guide-hero" id="top">
        <p className="guide-eyebrow">客户操作说明</p>
        <h1>从登录到读取商品，再到生成海报</h1>
        <p>
          按下面步骤操作即可。第一次使用需要安装电脑助手，并把 Ozon 店铺授权给这台电脑；以后只要打开网站和电脑助手，就可以继续读取商品。
        </p>
      </section>

      <div className="guide-layout">
        <aside className="guide-side" aria-label="页面导航">
          <strong>快速跳转</strong>
          <a href="#prepare">开始前准备</a>
          <a href="#setup">首次使用步骤</a>
          <a href="#work">读取商品和生成海报</a>
          <a href="#daily">日常使用</a>
          <a href="#faq">常见问题</a>
          <a href="#support">联系客服时提供什么</a>
        </aside>

        <div className="guide-content">
          <section className="guide-section" id="prepare">
            <h2>开始前准备</h2>
            <div className="guide-list">
              <GuideItem
                label="1. 一台常用电脑"
                text="建议使用固定办公电脑。电脑助手安装在这台电脑后，商品读取和海报生成会通过这台电脑完成。"
              />
              <GuideItem
                label="2. Ozon Seller API 信息"
                text="需要准备 Ozon Seller 后台里的 Client ID 和 API Key。它们用于读取你的店铺商品，不要发给无关人员。"
              />
              <GuideItem
                label="3. 一个可登录的 Ozon Rust Suite 账号"
                text="使用购买或客服开通时登记的邮箱、手机号登录。登录后才能下载电脑助手并绑定这台电脑。"
              />
            </div>
          </section>

          <section className="guide-section" id="setup">
            <h2>首次使用步骤</h2>
            <div className="guide-steps">
              <GuideStep index={1} title="打开网站并登录">
                <p>访问 ozon66.com，点击登录。按页面提示输入邮箱、手机号或账号信息。如果出现安全验证，请先完成验证。</p>
                <div className="guide-actions">
                  <a className="guide-button" href="/">
                    打开登录页
                  </a>
                </div>
              </GuideStep>
              <GuideStep index={2} title="确认服务已开通">
                <p>登录后查看页面左侧或顶部状态。如果显示“已开通”，可以继续下一步；如果显示“待确认”或“未开通”，请按页面提示提交开通申请或联系你的服务人员。</p>
              </GuideStep>
              <GuideStep index={3} title="下载安装电脑助手">
                <p>在“安装并打开电脑助手”步骤里，按你的电脑系统下载。</p>
                <ul>
                  <li>Windows 用户：优先下载 EXE 安装包；企业电脑也可以使用 MSI 安装包。</li>
                  <li>Mac 用户：下载 DMG，打开后把 Ozon Rust Local 拖到“应用程序”。</li>
                </ul>
              </GuideStep>
              <GuideStep index={4} title="打开电脑助手">
                <p>安装完成后打开 Ozon Rust Local。第一次打开时，系统可能会询问是否允许网络访问，请选择允许。打开后回到网站，点击“检测电脑助手”。</p>
              </GuideStep>
              <GuideStep index={5} title="授权这台电脑">
                <p>网站检测到电脑助手后，会出现“授权这台电脑”或“完成授权”。点击后等待页面显示“可以开始了”。这一步完成后，这台电脑就可以读取店铺商品。</p>
              </GuideStep>
              <GuideStep index={6} title="填写店铺授权信息">
                <p>在电脑助手里打开“店铺授权”，填写 Ozon Seller 的 Client ID 和 API Key，点击保存。保存后回到网站，点击刷新或重新检测。</p>
              </GuideStep>
            </div>
          </section>

          <section className="guide-section" id="work">
            <h2>读取商品和生成海报</h2>
            <div className="guide-steps">
              <GuideStep index={1} title="进入工作台">
                <p>网站显示“可以开始了”后，点击进入工作台。工作台会显示你的登录账号、电脑连接状态和店铺读取状态。</p>
              </GuideStep>
              <GuideStep index={2} title="读取商品列表">
                <p>点击“读取商品”。如果店铺授权正确，页面会显示商品数量和商品列表。商品较多时，第一次读取可能需要等几秒。</p>
              </GuideStep>
              <GuideStep index={3} title="查看商品详情和图片">
                <p>选择一个商品，或输入商品的 offer ID，点击读取详情。页面会展示商品名称、图片和可用于海报的基础信息。</p>
              </GuideStep>
              <GuideStep index={4} title="生成海报简报">
                <p>点击“生成海报简报”。系统会根据真实商品信息整理标题、卖点、图片参考和生成要求。生成前请确认商品图片和商品本身一致。</p>
              </GuideStep>
              <GuideStep index={5} title="选择图片生成方式">
                <p>建议优先使用已登录的龙虾、OpenClaw 或 Codex 继续生成图片。把海报简报复制过去后，按提示生成即可。</p>
                <p>如果你的账号已经开通后台图片生成服务，也可以选择后台自动生成。若页面提示“图片通道未开通”，请改用龙虾、OpenClaw 或 Codex 方式生成。</p>
              </GuideStep>
              <GuideStep index={6} title="检查成图">
                <p>海报生成后，请检查四件事：商品外观有没有变，包装文字有没有错，颜色和比例是否正常，卖点有没有夸大。如果不一致，重新生成或改简报后再生成。</p>
              </GuideStep>
            </div>
          </section>

          <section className="guide-section guide-note" id="daily">
            <h2>以后每天怎么用</h2>
            <p>正常情况下，只需要三步：</p>
            <ul>
              <li>打开 Ozon Rust Local 电脑助手。</li>
              <li>打开 ozon66.com 并登录。</li>
              <li>进入工作台，点击读取商品，选择商品生成海报简报。</li>
            </ul>
            <p>如果网站提示电脑未连接，先确认电脑助手已经打开，再点击“检测电脑助手”。</p>
          </section>

          <section className="guide-section" id="faq">
            <h2>常见问题</h2>
            <div className="guide-list">
              <GuideQuestion
                title="登录后看不到下一步怎么办？"
                text="先点击页面右上角“刷新”。如果仍然没有变化，可能是服务还没有完成开通确认，请联系服务人员。"
              />
              <GuideQuestion
                title="网站一直提示电脑助手未连接怎么办？"
                text="确认 Ozon Rust Local 已经打开。Windows 用户检查右下角托盘或开始菜单；Mac 用户检查“应用程序”里是否已经打开。如果刚安装完成，建议关闭浏览器页面后重新打开。"
              />
              <GuideQuestion
                title="保存 Ozon 信息后，仍然读不到商品怎么办？"
                text="请确认 Client ID 和 API Key 来自正确的 Ozon Seller 店铺，并且没有多复制空格。保存后回网站点击刷新，再试一次读取商品。"
              />
              <GuideQuestion
                title="能读取商品，但自动生成图片失败怎么办？"
                text="通常是图片生成通道没有开通。你仍然可以先使用“生成海报简报”，把简报复制到龙虾、OpenClaw 或 Codex 里生成图片。"
              />
              <GuideQuestion
                title="商品图片和成图不一致怎么办？"
                text="不要直接使用。重新生成时明确要求保留商品包装、颜色、文字和比例。如果仍然不一致，换一张更清晰的商品主图。"
              />
            </div>
          </section>

          <section className="guide-section guide-warning" id="support">
            <h2>联系客服时提供什么</h2>
            <p>遇到问题时，请把下面信息发给客服，能更快定位：</p>
            <ul>
              <li>登录账号，例如邮箱或手机号。</li>
              <li>电脑系统：Windows 或 Mac。</li>
              <li>卡在哪一步：登录、安装电脑助手、授权电脑、保存店铺信息、读取商品、生成海报。</li>
              <li>页面上的提示文字或截图。</li>
              <li>如果是商品问题，请提供 offer ID 或商品链接。</li>
            </ul>
            <p>不要把完整的 Ozon API Key、登录密码或验证码发到公开群里。客服需要排查时，会告诉你具体发哪些信息。</p>
          </section>
        </div>
      </div>

      <footer className="guide-footer">
        Ozon Rust Suite 使用说明。页面内容会随产品更新调整，请以 ozon66.com 当前页面提示为准。
      </footer>
    </main>
  );
}

function GuideItem({ label, text }: { label: string; text: string }) {
  return (
    <div className="guide-item">
      <span>{label}</span>
      <p>{text}</p>
    </div>
  );
}

function GuideQuestion({ title, text }: { title: string; text: string }) {
  return (
    <div className="guide-item">
      <h3>{title}</h3>
      <p>{text}</p>
    </div>
  );
}

function GuideStep({
  children,
  index,
  title
}: {
  children: ReactNode;
  index: number;
  title: string;
}) {
  return (
    <div className="guide-step">
      <span className="guide-number">{index}</span>
      <div>
        <h3>{title}</h3>
        {children}
      </div>
    </div>
  );
}
