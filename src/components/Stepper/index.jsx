import './style.css';

export default function Stepper({ currentStep, totalSteps, stepLabels }) {
  return (
    <div className="stepper-container">
      {/* 步骤指示器 */}
      <div className="stepper-steps">
        {stepLabels.map((label, index) => (
          <div key={index} className={`stepper-item ${index === currentStep ? 'active' : ''} ${index < currentStep ? 'completed' : ''}`}>
            <div className="stepper-circle">
              {index < currentStep ? '✓' : index + 1}
            </div>
            <div className="stepper-label">{label}</div>
          </div>
        ))}
      </div>

      {/* 进度条 */}
      <div className="stepper-progress-bar">
        <div
          className="stepper-progress-fill"
          style={{ width: `${(currentStep / (totalSteps - 1)) * 100}%` }}
        ></div>
      </div>
    </div>
  );
}
