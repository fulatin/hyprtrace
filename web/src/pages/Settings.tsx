import { useEffect, useMemo, useRef, useState } from 'react';
import { Skeleton, SkeletonPanel } from '../components/ui/Feedback';
import { useConfig } from '../lib/queries';
import { SECTIONS, SectionNav, type SectionId } from './settings/SectionNav';
import AiSection from './settings/AiSection';
import CategoriesSection from './settings/CategoriesSection';
import DangerSection from './settings/DangerSection';
import ExportSection from './settings/ExportSection';
import GoalsSection from './settings/GoalsSection';
import PrivacySection from './settings/PrivacySection';
import ProjectsSection from './settings/ProjectsSection';
import ServerSection from './settings/ServerSection';
import WeeklyReportSection from './settings/WeeklyReportSection';

/**
 * Settings shell: the sticky section rail with its scroll-spy, plus the gate
 * that used to keep the whole page behind a skeleton until the config request
 * settled. Every panel below owns its own data (react-query) and its own save
 * handler; the shell holds no field state.
 */
export default function Settings() {
  const config = useConfig();

  // Section rail: one ref per section, kept stable so scrolling only reads.
  const [activeSection, setActiveSection] = useState<SectionId>('status');
  const sectionRefs = useRef<Partial<Record<SectionId, HTMLElement | null>>>({});
  const setSectionRef = useMemo(() => {
    const map = {} as Record<SectionId, (el: HTMLElement | null) => void>;
    for (const s of SECTIONS) {
      map[s.id] = (el) => {
        sectionRefs.current[s.id] = el;
      };
    }
    return map;
  }, []);

  // Highlight whichever section sits at the top of the scrollport. The page
  // scrolls inside Layout's <main>, so listen in the capture phase instead of
  // assuming window scrolling.
  useEffect(() => {
    const compute = () => {
      let current: SectionId = SECTIONS[0].id;
      for (const s of SECTIONS) {
        const el = sectionRefs.current[s.id];
        if (el && el.getBoundingClientRect().top <= 160) current = s.id;
      }
      setActiveSection((prev) => (prev === current ? prev : current));
    };
    compute();
    document.addEventListener('scroll', compute, true);
    window.addEventListener('resize', compute);
    return () => {
      document.removeEventListener('scroll', compute, true);
      window.removeEventListener('resize', compute);
    };
  }, []);

  const scrollToSection = (id: SectionId) => {
    setActiveSection(id);
    const el = sectionRefs.current[id];
    if (!el) return;
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
  };

  // Same gate as before: nothing but the skeleton until `GET /api/config`
  // settles. A failed request settles too (config stays unset and the sections
  // render their empty fields), which is what the old `.finally()` did.
  if (config.isPending) {
    return (
      <div className="max-w-3xl space-y-6">
        <div className="space-y-2">
          <Skeleton className="h-6 w-32" />
          <Skeleton className="h-4 w-72" />
        </div>
        <SkeletonPanel height="h-24" />
        <SkeletonPanel height="h-56" />
        <SkeletonPanel height="h-40" />
      </div>
    );
  }

  return (
    <div className="max-w-5xl">
      <header className="mb-6">
        <h2 className="text-xl font-bold">Settings</h2>
        <p className="mt-1 text-sm text-fg-muted">
          AI providers, categories, projects, goals, reporting and local data controls.
        </p>
      </header>

      {/* Narrow screens: sticky top tabs. Desktop: sticky left rail (below). */}
      <SectionNav
        active={activeSection}
        onSelect={scrollToSection}
        layoutId="settings-tabs-pill"
        className="sticky top-0 z-20 mb-6 border-b border-line bg-bg/85 py-2 backdrop-blur-xl lg:hidden"
      />

      <div className="grid gap-6 lg:grid-cols-[12.5rem_minmax(0,1fr)] lg:gap-8">
        <SectionNav
          active={activeSection}
          onSelect={scrollToSection}
          layoutId="settings-rail-pill"
          className="hidden lg:sticky lg:top-4 lg:block lg:self-start"
        />

        <div className="min-w-0 space-y-6">
          <div ref={setSectionRef.status} className="scroll-mt-16 lg:scroll-mt-6">
            <ServerSection />
          </div>

          <div ref={setSectionRef.ai} className="scroll-mt-16 space-y-6 lg:scroll-mt-6">
            <AiSection />
          </div>

          <div ref={setSectionRef.categories} className="scroll-mt-16 lg:scroll-mt-6">
            <CategoriesSection />
          </div>

          <div ref={setSectionRef.goals} className="scroll-mt-16 lg:scroll-mt-6">
            <GoalsSection />
          </div>

          <div ref={setSectionRef.projects} className="scroll-mt-16 lg:scroll-mt-6">
            <ProjectsSection />
          </div>

          <div ref={setSectionRef.report} className="scroll-mt-16 lg:scroll-mt-6">
            <WeeklyReportSection />
          </div>

          <div ref={setSectionRef.export} className="scroll-mt-16 lg:scroll-mt-6">
            <ExportSection />
          </div>

          <div ref={setSectionRef.privacy} className="scroll-mt-16 lg:scroll-mt-6">
            <PrivacySection />
          </div>

          <div ref={setSectionRef.danger} className="scroll-mt-16 lg:scroll-mt-6">
            <DangerSection />
          </div>
        </div>
      </div>
    </div>
  );
}
