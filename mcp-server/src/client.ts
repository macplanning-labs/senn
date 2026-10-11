import {
  cloudflareAccessBlockMessage,
  isCloudflareAccessBlock,
} from "./cloudflare-access.js";

export class WipApiError extends Error {
  constructor(public status: number, public body: unknown) {
    super(WipApiError.extractMessage(status, body));
  }

  // ai_agent_api.rsの通常エラーは{"error": "..."}、429(tower_governorのレート制限層)は
  // {"detail": "リクエストが多すぎます。N秒後に再試行してください。"}と形式が異なる
  // どちらも拾えるようにする
  private static extractMessage(status: number, body: unknown): string {
    if (typeof body === "object" && body !== null) {
      const b = body as Record<string, unknown>;
      if (typeof b.error === "string") return b.error;
      if (typeof b.detail === "string") return b.detail;
    }
    // JSON でない（HTML）応答は、手前の Cloudflare Access が返したものかを見分ける（DEMO-000116）
    if (typeof body === "string" && isCloudflareAccessBlock(body)) {
      return cloudflareAccessBlockMessage(status);
    }
    return `SENN API error (HTTP ${status})`;
  }
}

export class WipClient {
  constructor(private baseUrl: string, private headers: Record<string, string>) {}

  /** 接続先の URL(端末の接続で credentials.env を新しく作るときに書く) */
  get apiBaseUrl(): string {
    return this.baseUrl;
  }

  private async request(
    method: string,
    path: string,
    opts?: {
      query?: Record<string, string | undefined>;
      json?: unknown;
      form?: FormData;
    }
  ): Promise<unknown> {
    // baseUrl + '/api/v1/ai-agent' + path にqueryを付与してfetch
    const url = new URL(`${this.baseUrl}/api/v1/ai-agent${path}`);

    // クエリパラメータの追加（undefinedはスキップ）
    if (opts?.query) {
      for (const [key, value] of Object.entries(opts.query)) {
        if (value !== undefined) {
          url.searchParams.set(key, value);
        }
      }
    }

    const requestHeaders: Record<string, string> = { ...this.headers };

    let body: string | FormData | undefined;

    // JSON送信の場合
    if (opts?.json) {
      requestHeaders["Content-Type"] = "application/json";
      body = JSON.stringify(opts.json);
    }

    // multipart送信の場合（Content-Typeは自動設定されるため手動設定しない）
    if (opts?.form) {
      body = opts.form;
    }

    const response = await fetch(url.toString(), {
      method,
      headers: requestHeaders,
      body,
    });

    // 204 No Content は null を返す
    if (response.status === 204) {
      return null;
    }

    // レスポンステキストを先に読む（ボディは一度だけ読める）
    const responseText = await response.text();

    // status >= 400 の場合、bodyをJSON.parse試行して WipApiError を throw
    if (response.status >= 400) {
      let errorBody: unknown;
      try {
        errorBody = JSON.parse(responseText);
      } catch {
        errorBody = responseText;
      }
      throw new WipApiError(response.status, errorBody);
    }

    // 200-399: JSON解析して返す
    try {
      return JSON.parse(responseText);
    } catch {
      return responseText;
    }
  }

  // ============ プロジェクト ============
  async listProjects(): Promise<unknown> {
    return this.request("GET", "/projects/");
  }

  async createProject(body: {
    name: string;
    prefix: string;
    description?: string;
    team_ids: number[];
  }): Promise<unknown> {
    return this.request("POST", "/projects/", { json: body });
  }

  // ============ チケット ============
  async listTickets(
    projectPrefix: string,
    page?: number,
    filters?: {
      status?: string[];
      status_not?: string[];
      label?: string;
      cycle?: number;
      assignee?: string;
      updated_after?: string;
    }
  ): Promise<unknown> {
    const query: Record<string, string | undefined> = { project_prefix: projectPrefix };
    if (page !== undefined) {
      query.page = String(page);
    }
    if (filters) {
      if (filters.status?.length) query.status = filters.status.join(",");
      if (filters.status_not?.length) query.status_not = filters.status_not.join(",");
      query.label = filters.label;
      if (filters.cycle !== undefined) query.cycle = String(filters.cycle);
      query.assignee = filters.assignee;
      query.updated_after = filters.updated_after;
    }
    return this.request("GET", "/tickets/", { query });
  }

  async getTicket(ticketKey: string): Promise<unknown> {
    return this.request("GET", `/tickets/${ticketKey}/`);
  }

  async createTicket(body: {
    project_prefix?: string;
    title: string;
    description?: string;
    priority?: string;
    ticket_type?: string;
    due_date?: string;
    labels?: string[];
    assignee_usernames?: string[];
    team_id?: number;
    team_slug?: string;
    status?: string;
    story_points?: number;
    cycle?: number;
    parent_key?: string;
    start_date?: string;
  }): Promise<unknown> {
    return this.request("POST", "/tickets/", { json: body });
  }

  async bulkCreateTickets(body: Record<string, unknown>): Promise<unknown> {
    return this.request("POST", "/tickets/bulk/", { json: body });
  }

  async patchTicket(ticketKey: string, body: Record<string, unknown>): Promise<unknown> {
    return this.request("PATCH", `/tickets/${ticketKey}/`, { json: body });
  }

  async deleteTicket(ticketKey: string, reason: string): Promise<unknown> {
    return this.request("DELETE", `/tickets/${ticketKey}/`, {
      query: { reason },
    });
  }

  async addTicketComment(ticketKey: string, body: { body: string }): Promise<unknown> {
    return this.request("POST", `/tickets/${ticketKey}/comments/`, { json: body });
  }

  async updateTicketComment(
    ticketKey: string,
    commentId: number,
    body: { body: string }
  ): Promise<unknown> {
    return this.request("PATCH", `/tickets/${ticketKey}/comments/${commentId}/`, { json: body });
  }

  // ============ 依存関係 ============
  async listTicketDependencies(ticketKey: string): Promise<unknown> {
    return this.request("GET", `/tickets/${ticketKey}/dependencies/`);
  }

  async addTicketDependency(
    ticketKey: string,
    body: {
      to_task_key: string;
      dependency_type?: string;
    }
  ): Promise<unknown> {
    return this.request("POST", `/tickets/${ticketKey}/dependencies/`, { json: body });
  }

  async deleteTicketDependency(ticketKey: string, dependencyId: number): Promise<unknown> {
    return this.request("DELETE", `/tickets/${ticketKey}/dependencies/${dependencyId}/`);
  }

  async getProjectDependencyGraph(projectPrefix: string): Promise<unknown> {
    return this.request("GET", `/projects/${projectPrefix}/dependencies/`);
  }

  // ============ チケット参照リンク ============
  async listTicketLinks(ticketKey: string): Promise<unknown> {
    return this.request("GET", `/tickets/${ticketKey}/links/`);
  }

  async addTicketLink(
    ticketKey: string,
    body: {
      url: string;
      title?: string;
    }
  ): Promise<unknown> {
    return this.request("POST", `/tickets/${ticketKey}/links/`, { json: body });
  }

  async deleteTicketLink(ticketKey: string, linkId: number): Promise<unknown> {
    return this.request("DELETE", `/tickets/${ticketKey}/links/${linkId}/`);
  }

  // ============ Cycle ============
  async listCycles(opts?: {
    projectPrefix?: string;
    status?: string;
  }): Promise<unknown> {
    return this.request("GET", "/cycles/", {
      query: {
        project_prefix: opts?.projectPrefix,
        status: opts?.status,
      },
    });
  }

  async createCycle(body: {
    project_prefix: string;
    name: string;
    description?: string;
    start_date: string;
    end_date: string;
    status?: string;
  }): Promise<unknown> {
    return this.request("POST", "/cycles/", { json: body });
  }

  async getCycle(cycleId: number): Promise<unknown> {
    return this.request("GET", `/cycles/${cycleId}/`);
  }

  async patchCycle(cycleId: number, body: Record<string, unknown>): Promise<unknown> {
    return this.request("PATCH", `/cycles/${cycleId}/`, { json: body });
  }

  async deleteCycle(cycleId: number): Promise<unknown> {
    return this.request("DELETE", `/cycles/${cycleId}/`);
  }

  // ============ Team ============
  async listTeams(): Promise<unknown> {
    return this.request("GET", "/teams/");
  }

  async createTeam(body: {
    name: string;
    slug?: string;
    description?: string;
    icon?: string;
    color?: string;
    is_active?: boolean;
    prefix?: string;
  }): Promise<unknown> {
    return this.request("POST", "/teams/", { json: body });
  }

  /** team は ID か slug */
  async updateTeam(
    team: string,
    body: {
      name?: string;
      description?: string;
      icon?: string;
      color?: string;
      prefix?: string;
      is_active?: boolean;
    }
  ): Promise<unknown> {
    return this.request("PATCH", `/teams/${encodeURIComponent(team)}/`, { json: body });
  }

  // ============ 端末の接続(コピー・貼り付けなし。DEMO-000192 フェーズ4) ============
  /** 承認されたあと、メモリ上のキーを入れ替える(再起動なしで、新しいキーで動く) */
  setApiKey(key: string): void {
    this.headers["X-AI-Api-Key"] = key;
  }

  async startPairing(body: {
    keyHash: string;
    keyPrefix: string;
    label: string;
  }): Promise<{ code: string; deviceSecret: string; url: string; expiresInSeconds: number }> {
    return (await this.request("POST", "/pairing/", { json: body })) as {
      code: string;
      deviceSecret: string;
      url: string;
      expiresInSeconds: number;
    };
  }

  async pairingStatus(
    code: string,
    deviceSecret: string
  ): Promise<{ status: string; message: string | null }> {
    return (await this.request("POST", "/pairing/status/", {
      json: { code, deviceSecret },
    })) as { status: string; message: string | null };
  }

  // ============ Wiki ============
  async listWikiPages(projectPrefix: string): Promise<unknown> {
    return this.request("GET", "/wiki-pages/", {
      query: { project_prefix: projectPrefix },
    });
  }

  async searchWikiPages(
    projectPrefix: string | undefined,
    opts: { search?: string; category?: string; page?: number }
  ): Promise<unknown> {
    return this.request("GET", "/wiki-search/", {
      query: {
        project_prefix: projectPrefix,
        search: opts.search,
        category: opts.category,
        page: opts.page === undefined ? undefined : String(opts.page),
      },
    });
  }

  async getWikiPage(slug: string, projectPrefix: string | undefined): Promise<unknown> {
    return this.request("GET", `/wiki-pages/${encodeURIComponent(slug)}/`, {
      query: { project_prefix: projectPrefix },
    });
  }

  async listWikiRevisions(slug: string, projectPrefix: string | undefined): Promise<unknown> {
    return this.request("GET", `/wiki-pages/${encodeURIComponent(slug)}/revisions/`, {
      query: { project_prefix: projectPrefix },
    });
  }

  async createWikiPage(body: {
    project_prefix?: string;
    title: string;
    category?: string;
    content?: string;
  }): Promise<unknown> {
    return this.request("POST", "/wiki-pages/", { json: body });
  }

  async updateWikiPage(
    slug: string,
    projectPrefix: string | undefined,
    body: { title?: string; category?: string; content?: string; revision_comment?: string }
  ): Promise<unknown> {
    return this.request("PATCH", `/wiki-pages/${encodeURIComponent(slug)}/`, {
      query: { project_prefix: projectPrefix },
      json: body,
    });
  }

  async deleteWikiPage(slug: string, projectPrefix: string | undefined): Promise<unknown> {
    return this.request("DELETE", `/wiki-pages/${encodeURIComponent(slug)}/`, {
      query: { project_prefix: projectPrefix },
    });
  }

  async listWikiAttachments(slug: string, projectPrefix: string): Promise<unknown> {
    return this.request("GET", `/wiki-pages/${slug}/attachments/`, {
      query: { project_prefix: projectPrefix },
    });
  }

  async uploadWikiAttachment(
    slug: string,
    projectPrefix: string,
    form: FormData
  ): Promise<unknown> {
    return this.request("POST", `/wiki-pages/${slug}/attachments/`, {
      form,
      query: { project_prefix: projectPrefix },
    });
  }

  async replaceWikiAttachment(slug: string, attachmentId: number, form: FormData): Promise<unknown> {
    return this.request("PUT", `/wiki-pages/${slug}/attachments/${attachmentId}/`, { form });
  }

  async deleteWikiAttachment(slug: string, attachmentId: number): Promise<unknown> {
    return this.request("DELETE", `/wiki-pages/${slug}/attachments/${attachmentId}/`);
  }

  // ============ AI Run ============
  async aiRunStart(body: {
    ticket_key: string;
    run_uuid: string;
    tool?: string;
    model?: string;
    trigger_type?: string;
  }): Promise<unknown> {
    return this.request("POST", `/runs/${body.run_uuid}/start/`, { json: body });
  }

  async aiRunFinish(body: {
    run_uuid: string;
    result: string;
    summary?: string;
    input_tokens?: number;
    output_tokens?: number;
    cache_read_tokens?: number;
    cache_creation_tokens?: number;
    usd_equivalent?: number;
  }): Promise<unknown> {
    return this.request("POST", `/runs/${body.run_uuid}/finish/`, { json: body });
  }

  async aiRunError(body: {
    run_uuid: string;
    detail: string;
    ticket_key?: string;
  }): Promise<unknown> {
    return this.request("POST", `/runs/${body.run_uuid}/error/`, { json: body });
  }
}
