import { useEffect, useState, useCallback } from 'react';
import { toast } from 'react-toastify';
import { all_goose_modes, ModeSelectionItem } from './ModeSelectionItem';
import { useConfig } from '../../ConfigContext';
import { ConversationLimitsDropdown } from './ConversationLimitsDropdown';
import { updateSession } from '../../../api';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  restartNeeded: {
    id: 'modeSection.restartNeeded',
    defaultMessage:
      'Mode changed. Other active sessions should be restarted for the change to take effect.',
  },
});

export const ModeSection = ({ sessionId }: { sessionId?: string }) => {
  const [currentMode, setCurrentMode] = useState('auto');
  const [maxTurns, setMaxTurns] = useState<number>(1000);
  const { config, read, upsert } = useConfig();
  const intl = useIntl();

  const handleModeChange = async (newMode: string) => {
    if (currentMode === newMode) {
      return;
    }

    try {
      if (sessionId) {
        await updateSession({ body: { session_id: sessionId, goose_mode: newMode } });
      }
      await upsert('GOOSE_MODE', newMode, false);
      setCurrentMode(newMode);

      // The session-scoped updateSession call above already applies the change
      // live to *this* session; other already-open sessions still read the
      // stale mode from their own in-memory config until restarted.
      toast.info(intl.formatMessage(i18n.restartNeeded), {
        position: 'top-right',
        autoClose: 5000,
        hideProgressBar: true,
        closeOnClick: true,
        pauseOnHover: true,
      });
    } catch (error) {
      console.error('Error updating goose mode:', error);
      throw new Error(`Failed to store new goose mode: ${newMode}`);
    }
  };

  useEffect(() => {
    const mode = config.GOOSE_MODE as string | undefined;
    if (mode) {
      setCurrentMode(mode);
    }
  }, [config.GOOSE_MODE]);

  const fetchMaxTurns = useCallback(async () => {
    try {
      const turns = (await read('GOOSE_MAX_TURNS', false)) as number;
      if (turns) {
        setMaxTurns(turns);
      }
    } catch (error) {
      console.error('Error fetching max turns:', error);
    }
  }, [read]);

  const handleMaxTurnsChange = async (value: number) => {
    try {
      await upsert('GOOSE_MAX_TURNS', value, false);
      setMaxTurns(value);
    } catch (error) {
      console.error('Error updating max turns:', error);
    }
  };

  useEffect(() => {
    fetchMaxTurns();
  }, [fetchMaxTurns]);

  return (
    <div className="space-y-1">
      {/* Mode Selection */}
      {all_goose_modes.map((mode) => (
        <ModeSelectionItem
          key={mode.key}
          mode={mode}
          currentMode={currentMode}
          showDescription={true}
          isApproveModeConfigure={false}
          handleModeChange={handleModeChange}
        />
      ))}

      {/* Conversation Limits Dropdown */}
      <ConversationLimitsDropdown maxTurns={maxTurns} onMaxTurnsChange={handleMaxTurnsChange} />
    </div>
  );
};
