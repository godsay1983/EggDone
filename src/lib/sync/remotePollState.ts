export interface RemotePollTicket {
  generation: number;
  syncSequence: number;
}

// Probe observations are not upload receipts. Reset on every configuration save.
export class RemotePollState {
  private generation: number = 0;
  private syncSequence: number = 0;
  private ruleToken: string | null = null;
  private linkToken: string | null = null;

  reset(): void {
    this.generation += 1;
    this.ruleToken = null;
    this.linkToken = null;
  }

  beginSync(): number {
    this.syncSequence += 1;
    return this.generation;
  }

  invalidateProbe(): void { this.syncSequence += 1; }

  capture(): RemotePollTicket {
    return { generation: this.generation, syncSequence: this.syncSequence };
  }

  isCurrent(ticket: RemotePollTicket): boolean {
    return this.isGenerationCurrent(ticket.generation) && ticket.syncSequence === this.syncSequence;
  }

  isGenerationCurrent(generation: number): boolean { return generation === this.generation; }

  rulesChanged(token: string): boolean { return token !== this.ruleToken; }
  linksChanged(token: string | undefined): boolean { return token !== undefined && token !== this.linkToken; }
  acknowledgeLinks(generation: number, token: string): void {
    if (this.isGenerationCurrent(generation)) this.linkToken = token;
  }

  acknowledgeRules(generation: number, token: string): void {
    if (this.isGenerationCurrent(generation)) this.ruleToken = token;
  }
}
