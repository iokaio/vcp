import { send } from '../infrastructure/mail';
export function accept(id: string) { return send(id); }
